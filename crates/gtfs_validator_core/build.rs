use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let src_dir = manifest_dir.join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    let const_re =
        Regex::new(r#"(?m)^\s*(?:pub\s+)?const\s+([A-Z0-9_]+)\s*:\s*&str\s*=\s*"([^"]+)";"#)
            .expect("const regex");
    let assign_re = Regex::new(
        r#"(?s)let\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*ValidationNotice::new\(\s*([^,]+?)\s*,\s*NoticeSeverity::(Error|Warning|Info)"#,
    )
    .expect("assign regex");
    let chained_location_re = Regex::new(
        r#"(?s)ValidationNotice::new\(\s*([^,]+?)\s*,\s*NoticeSeverity::(Error|Warning|Info).*?\)\s*\.with_location\b"#,
    )
    .expect("chained location regex");
    let code_severity_re =
        Regex::new(r#"(?s)\b([A-Z0-9_]+)\s*,\s*NoticeSeverity::(Error|Warning|Info)"#)
            .expect("code severity regex");
    let literal_severity_re =
        Regex::new(r#"(?s)"([^"]+)"\s*,\s*NoticeSeverity::(Error|Warning|Info)"#)
            .expect("literal severity regex");
    let struct_re = Regex::new(
        r#"(?s)code:\s*([A-Z0-9_]+)\.to_string\(\)\s*,\s*severity:\s*NoticeSeverity::(Error|Warning|Info)"#,
    )
    .expect("struct regex");
    let item_re = Regex::new(r#""([^"]+)""#).expect("field order item regex");

    let mut const_map = HashMap::new();
    let mut contents_by_file = Vec::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let contents = fs::read_to_string(&path).expect("read source");
        for caps in const_re.captures_iter(&contents) {
            let name = caps[1].to_string();
            let value = caps[2].to_string();
            const_map.insert(name, value);
        }
        contents_by_file.push(contents);
    }

    let mut entries: BTreeMap<String, (String, BTreeMap<String, String>)> = BTreeMap::new();
    for contents in &contents_by_file {
        for caps in literal_severity_re.captures_iter(contents) {
            let code = caps[1].to_string();
            let severity = caps[2].to_string();
            entries
                .entry(code)
                .or_insert_with(|| (severity, BTreeMap::new()));
        }
        for caps in code_severity_re.captures_iter(contents) {
            let name = caps[1].to_string();
            let severity = caps[2].to_string();
            if let Some(code) = const_map.get(&name).cloned() {
                entries
                    .entry(code)
                    .or_insert_with(|| (severity, BTreeMap::new()));
            }
        }
        for caps in assign_re.captures_iter(contents) {
            let var_name = caps[1].to_string();
            let code_expr = caps[2].trim();
            let severity = caps[3].to_string();
            let Some(code) = resolve_code_expr(code_expr, &const_map) else {
                continue;
            };
            let entry = entries
                .entry(code.clone())
                .or_insert_with(|| (severity.clone(), BTreeMap::new()));
            entry.0 = severity.clone();

            let match_pos = caps.get(0).map(|m| m.end()).unwrap_or(0);
            let end_pos = find_end_pos(contents, match_pos, &var_name);
            let slice = &contents[match_pos..end_pos];
            let insert_re = Regex::new(&format!(
                r#"\b{}\.(?:insert_context_field|with_context_field)\(\s*"([^"]+)"\s*,"#,
                regex::escape(&var_name)
            ))
            .expect("insert regex");
            for field_caps in insert_re.captures_iter(slice) {
                let field_name = field_caps[1].to_string();
                let start = field_caps.get(0).map(|m| m.end()).unwrap_or(0);
                if let Some((expr, _)) = extract_argument(slice, start) {
                    let field_type = infer_type(&field_name, &expr).to_string();
                    merge_field_type(&mut entry.1, field_name, field_type);
                } else {
                    let field_type = infer_type(&field_name, "").to_string();
                    merge_field_type(&mut entry.1, field_name, field_type);
                }
            }

            let location_re = Regex::new(&format!(
                r#"\b{}\.(?:set_location|with_location)\("#,
                regex::escape(&var_name)
            ))
            .expect("location regex");
            if location_re.is_match(slice) {
                insert_location_fields(&mut entry.1);
            }

            let field_order_re = Regex::new(&format!(
                r#"(?s)\b{}\.\s*field_order\s*=\s*vec!\[(?P<items>.*?)\]"#,
                regex::escape(&var_name)
            ))
            .expect("field order regex");

            for order_caps in field_order_re.captures_iter(slice) {
                let items = order_caps.name("items").map(|m| m.as_str()).unwrap_or("");
                for item_caps in item_re.captures_iter(items) {
                    let field_name = item_caps[1].to_string();
                    let field_type = infer_type(&field_name, "").to_string();
                    merge_field_type(&mut entry.1, field_name, field_type);
                }
            }
        }
        for caps in chained_location_re.captures_iter(contents) {
            let code_expr = caps[1].trim();
            let severity = caps[2].to_string();
            let Some(code) = resolve_code_expr(code_expr, &const_map) else {
                continue;
            };
            let entry = entries
                .entry(code)
                .or_insert_with(|| (severity.clone(), BTreeMap::new()));
            entry.0 = severity;
            insert_location_fields(&mut entry.1);
        }
        for caps in struct_re.captures_iter(contents) {
            let name = caps[1].to_string();
            let severity = caps[2].to_string();
            if let Some(code) = const_map.get(&name).cloned() {
                entries
                    .entry(code)
                    .or_insert_with(|| (severity, BTreeMap::new()));
            }
        }
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let dest_path = out_dir.join("notice_schema_data.rs");
    let mut output = String::new();
    output.push_str("pub static NOTICE_SCHEMA_ENTRIES: &[NoticeSchemaEntry] = &[\n");
    for (code, (severity, fields)) in entries {
        let mut field_list: Vec<_> = fields.into_iter().collect();
        field_list.sort_by(|a, b| a.0.cmp(&b.0));
        let fields_literal = if field_list.is_empty() {
            "&[]".to_string()
        } else {
            let joined = field_list
                .iter()
                .map(|(field, field_type)| {
                    format!(
                        "NoticeSchemaFieldEntry {{ name: \"{}\", field_type: \"{}\" }}",
                        field, field_type
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("&[{}]", joined)
        };
        output.push_str(&format!(
            "    NoticeSchemaEntry {{ code: \"{}\", severity: NoticeSchemaSeverity::{} , fields: {} }},\n",
            code, severity, fields_literal
        ));
    }
    output.push_str("];\n");
    fs::write(&dest_path, output).expect("write notice schema data");
}

fn resolve_code_expr(expr: &str, const_map: &HashMap<String, String>) -> Option<String> {
    let trimmed = expr.trim();
    if let Some(code) = trimmed
        .strip_prefix('"')
        .and_then(|val| val.strip_suffix('"'))
    {
        return Some(code.to_string());
    }
    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return const_map.get(trimmed).cloned();
    }
    None
}

fn find_end_pos(contents: &str, start: usize, var_name: &str) -> usize {
    let push_re = Regex::new(&format!(
        r#"\b\w+\.push\(\s*{}\s*\)"#,
        regex::escape(var_name)
    ))
    .expect("push regex");
    let return_re =
        Regex::new(&format!(r#"\breturn\s+{}\b"#, regex::escape(var_name))).expect("return regex");
    let reassign_re = Regex::new(&format!(
        r#"\blet\s+(?:mut\s+)?{}\s*="#,
        regex::escape(var_name)
    ))
    .expect("reassign regex");
    let fn_re = Regex::new(r#"(?m)^\s*(?:pub\s+)?fn\s+\w+"#).expect("fn regex");
    let impl_re = Regex::new(r#"(?m)^\s*impl\b"#).expect("impl regex");

    let mut end = contents.len();
    for re in [&push_re, &return_re, &reassign_re, &fn_re, &impl_re] {
        if let Some(found) = re.find_at(contents, start) {
            end = end.min(found.start());
        }
    }
    end
}

fn extract_argument(contents: &str, start: usize) -> Option<(String, usize)> {
    let mut i = start;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    while i < contents.len() {
        let ch = contents.as_bytes()[i] as char;
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else {
            match ch {
                '"' => in_string = true,
                '(' => depth += 1,
                ')' => {
                    if depth == 0 {
                        let expr = contents[start..i].trim().to_string();
                        return Some((expr, i));
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

fn merge_field_type(fields: &mut BTreeMap<String, String>, field: String, field_type: String) {
    match fields.get(&field) {
        None => {
            fields.insert(field, field_type);
        }
        Some(existing) if existing == "object" => {
            fields.insert(field, field_type);
        }
        Some(existing) if existing == &field_type => {}
        Some(_) => {
            fields.insert(field, "object".to_string());
        }
    }
}

fn insert_location_fields(fields: &mut BTreeMap<String, String>) {
    merge_field_type(fields, "filename".to_string(), "string".to_string());
    merge_field_type(fields, "fieldName".to_string(), "string".to_string());
    merge_field_type(fields, "csvRowNumber".to_string(), "integer".to_string());
}

fn infer_type(field_name: &str, expr: &str) -> &'static str {
    let expr = expr.trim();
    let name_hint = type_from_name(field_name);

    if expr.is_empty() {
        return name_hint;
    }
    if expr == "true" || expr == "false" {
        return "boolean";
    }
    if expr.starts_with('"') {
        return "string";
    }
    if expr.starts_with("format!") || expr.contains(".to_string()") {
        return "string";
    }
    if expr.starts_with("String::") {
        return "string";
    }
    if expr.contains("Value::Null") || expr == "None" {
        return name_hint;
    }
    if is_numeric_literal(expr) {
        if expr.contains('.') || expr.contains('e') || expr.contains('E') {
            return "number";
        }
        return "integer";
    }

    name_hint
}

fn type_from_name(name: &str) -> &'static str {
    let lowered = name.to_ascii_lowercase();
    if matches!(lowered.as_str(), "match" | "match1" | "match2") {
        return "array<number,2>";
    }
    if lowered.contains("lat") || lowered.contains("lon") {
        return "number";
    }
    if lowered.contains("distance")
        || lowered.contains("speed")
        || lowered.contains("ratio")
        || lowered.contains("percent")
    {
        return "number";
    }
    if lowered.contains("index")
        || lowered.contains("row")
        || lowered.contains("number")
        || lowered.contains("sequence")
        || lowered.contains("count")
    {
        return "integer";
    }
    if lowered.contains("id")
        || lowered.contains("name")
        || lowered.contains("filename")
        || lowered.contains("field")
        || lowered.contains("type")
    {
        return "string";
    }
    "object"
}

fn is_numeric_literal(expr: &str) -> bool {
    let numeric_re = Regex::new(r#"^-?[0-9][0-9_]*(\.[0-9_]+)?([eE][+-]?[0-9_]+)?[a-zA-Z0-9_]*$"#)
        .expect("numeric regex");
    numeric_re.is_match(expr)
}

fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs")
            && path.file_name().and_then(|name| name.to_str()) != Some("validator.rs")
        {
            files.push(path);
        }
    }
}
