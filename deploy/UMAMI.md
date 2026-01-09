# 📈 Getting Started with Umami Analytics

Umami is a simple, fast, privacy-focused alternative to Google Analytics. We have integrated it into your Docker stack.

## 1. Deploy Changes

Deploy the new configuration to your server:

```bash
# If running locally or via SSH
git pull
docker compose pull
docker compose up -d
```

This will start:

- `umami-db` (PostgreSQL)
- `umami` (Analytics interface on port 3000 -> 3001 internally, but proxied via Caddy)

## 2. Configure DNS

Ensure you have a DNS record for your analytics subdomain:

```
analytics.gtfs.guru  →  YOUR_SERVER_IP
```

## 3. Initial Setup

1. Open `https://analytics.gtfs.guru` in your browser.
2. Login with the default credentials:
   - **Username:** `admin`
   - **Password:** `umami`
3. **Important:** Change your password immediately.
4. Go to **Websites** -> **Add Website**.
   - **Name:** GTFS Guru
   - **Domain:** gtfs.guru
   - **Enable share URL:** (Optional, if you want public stats)

## 4. Get Tracking Code

1. Click on the **Edit** button (pencil icon) for your new website.
2. Go to the **Tracking Code** tab.
3. Copy the `<script>` tag. It will look something like this:

```html
<script defer src="https://analytics.gtfs.guru/script.js" data-website-id="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"></script>
```

## 5. Add to Website

Paste the tracking code into the `<head>` section of `crates/gtfs_validator_gui/frontend/index.html`:

```html
<head>
    ...
    <script defer src="https://analytics.gtfs.guru/script.js" data-website-id="YOUR-ID"></script>
    ...
</head>
```

Redeploy your website (if needed) or just the frontend, and you'll start seeing data!
