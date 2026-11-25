# StorByte API

An optional Phase-1 control-plane scaffold for StorByte. It does **not** proxy
Google Drive file bytes: file reads and streaming belong to the desktop app.
Only health reporting is implemented; the auth, user, device and licence
endpoints below deliberately return `501 Not Implemented`.

## Run locally

Install Go 1.22 or newer. From the repository root, start PostgreSQL with Docker
Compose, then run the Go process separately (Compose does not start the API):

```sh
docker compose up -d postgres
cd services/api
cp .env.example .env
go run ./cmd/server
```

On PowerShell, use `Copy-Item .env.example .env` for the copy step. The defaults
match the local PostgreSQL service in the root `docker-compose.yml`. Existing
values in the process environment take precedence over `.env`.

The server attempts to connect to PostgreSQL and apply the SQL migrations on
startup. A connection failure is logged and the API still starts with degraded
health; migration failures are logged as warnings. Stop the process with Ctrl+C.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `STORBYTE_API_ADDR` | `:8080` | HTTP listen address |
| `DATABASE_URL` | `postgres://storbyte:storbyte@localhost:5432/storbyte?sslmode=disable` | Local prototype PostgreSQL connection |
| `STROEX_MIGRATIONS_DIR` | Auto-detected `migrations` directory | Optional SQL migration-directory override; the Go helper currently retains this legacy variable name |

## Registered routes

| Method | Route | Phase-1 status |
| --- | --- | --- |
| GET | `/health` | Implemented; HTTP 200 with database connectivity information |
| POST | `/auth/register` | Stub; HTTP 501 |
| POST | `/auth/login` | Stub; HTTP 501 |
| GET | `/user` | Stub; HTTP 501 |
| GET | `/devices` | Stub; HTTP 501 |
| POST | `/devices` | Stub; HTTP 501 |
| DELETE | `/devices/{id}` | Stub; HTTP 501 |
| GET | `/license` | Stub; HTTP 501 |
| POST | `/license/activate` | Stub; HTTP 501 |

For example, `curl http://localhost:8080/health` returns JSON shaped like:

```json
{
  "status": "ok",
  "service": "storbyte-api",
  "time": "2026-01-01T00:00:00Z",
  "database": true,
  "fileProxy": false
}
```

The timestamp is generated for each request in UTC. A missing pool or failed
database ping changes `status` to `"degraded"` and `database` to `false`, while
HTTP status remains **200** and `fileProxy` stays **false**. Consumers that need
database readiness must inspect those JSON fields; this is not a schema or
migration-readiness check.

Every stub returns `{"error":"not implemented in Phase 1 prototype scaffold"}`.
It does not authenticate a user, register a device or activate a licence.

## Tests

From `services/api`:

```sh
go test ./...
go test -race ./...
go vet ./...
```

The table-driven HTTP tests use `httptest` and a small database-ping stub. They
exercise healthy, missing/failed-database and cancelled-request health paths,
all eight registered stubs, wrong methods and unknown paths without starting
PostgreSQL, contacting Google or requiring Windows/WinFsp. They do not claim to
test real database connectivity, migrations or desktop filesystem behavior.
