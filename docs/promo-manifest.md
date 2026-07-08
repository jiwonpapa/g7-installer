# Promo Manifest Policy

G7 Installer can show a small recommendation area in the left wizard rail. The
content is loaded from a JSON manifest so paid/free tools can be announced
without shipping a new binary.

## Rules

- The manifest is JSON only. Raw HTML is not accepted.
- Links must use `https://` or `http://`.
- The renderer escapes text and adds `rel="noreferrer noopener"`.
- The UI shows at most three slots.
- Users can dismiss the current manifest version in the browser.
- Set the page meta value to `off`, `disabled`, or `none` to disable remote
  promos.

## Schema

```json
{
  "version": 1,
  "updated_at": "2026-07-08",
  "slots": [
    {
      "id": "server-care-pro",
      "title": "Server Care Pro",
      "body": "백업, 점검, 장애 알림을 한 화면에서 관리하는 운영 도구",
      "badge": "유료 예정",
      "href": "https://g7devops.com",
      "cta": "계획 보기",
      "theme": "pro"
    }
  ]
}
```

## Field Limits

| Field | Limit |
| --- | --- |
| `title` | 34 characters |
| `body` | 88 characters |
| `badge` | 12 characters |
| `cta` | 18 characters |
| `theme` | `default`, `github`, or `pro` |

Use this only for owner-controlled announcements. Do not load third-party ad
scripts inside the installer.
