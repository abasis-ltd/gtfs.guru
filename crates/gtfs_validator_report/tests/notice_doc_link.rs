use gtfs_guru_core::{NoticeContainer, NoticeSeverity, ValidationNotice};
use gtfs_guru_report::{
    generate_html_report_string, HtmlReportContext, ReportSummary, ReportSummaryContext,
};

#[test]
fn notice_group_links_to_gtfs_guru_guide() {
    let mut notices = NoticeContainer::new();
    notices.push(ValidationNotice::new(
        "missing_required_field",
        NoticeSeverity::Error,
        "required value is missing",
    ));
    let summary = ReportSummary::from_context(ReportSummaryContext::new());

    let html = generate_html_report_string(
        &notices,
        &summary,
        HtmlReportContext::from_summary(&summary, "test.zip"),
    );

    assert!(html.contains("href=\"https://gtfs.guru/notices/missing_required_field/\""));
    assert!(!html.contains("gtfs-validator.mobilitydata.org/rules.html#"));
}
