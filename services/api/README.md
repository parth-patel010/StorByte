# Stroex API

Control-plane only. Does **not** proxy Google Drive file bytes.

```powershell
docker compose up -d   # from repo root
cd services/api
go run ./cmd/server
```

`GET /health` → `{ "status": "ok", "fileProxy": false }`
