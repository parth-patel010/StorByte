package httpapi

import (
        "encoding/json"
        "net/http"
        "time"

        "github.com/jackc/pgx/v5/pgxpool"
)

type Router struct {
        pool *pgxpool.Pool
        mux  *http.ServeMux
}

func NewRouter(pool *pgxpool.Pool) http.Handler {
        r := &Router{pool: pool, mux: http.NewServeMux()}
        r.routes()
        return r.mux
}

func (r *Router) routes() {
        r.mux.HandleFunc("GET /health", r.health)
        r.mux.HandleFunc("POST /auth/register", r.notImplemented)
        r.mux.HandleFunc("POST /auth/login", r.notImplemented)
        r.mux.HandleFunc("GET /user", r.notImplemented)
        r.mux.HandleFunc("GET /devices", r.notImplemented)
        r.mux.HandleFunc("POST /devices", r.notImplemented)
        r.mux.HandleFunc("DELETE /devices/{id}", r.notImplemented)
        r.mux.HandleFunc("GET /license", r.notImplemented)
        r.mux.HandleFunc("POST /license/activate", r.notImplemented)
}

func (r *Router) health(w http.ResponseWriter, req *http.Request) {
        status := "ok"
        dbOK := false
        if r.pool != nil {
                if err := r.pool.Ping(req.Context()); err == nil {
                        dbOK = true
                } else {
                        status = "degraded"
                }
        } else {
                status = "degraded"
        }
        writeJSON(w, http.StatusOK, map[string]any{
                "status":    status,
                "service":   "stroex-api",
                "time":      time.Now().UTC().Format(time.RFC3339),
                "database":  dbOK,
                "fileProxy": false,
        })
}

func (r *Router) notImplemented(w http.ResponseWriter, _ *http.Request) {
        writeJSON(w, http.StatusNotImplemented, map[string]string{
                "error": "not implemented in Phase 1 prototype scaffold",
        })
}

func writeJSON(w http.ResponseWriter, code int, v any) {
        w.Header().Set("Content-Type", "application/json")
        w.WriteHeader(code)
        _ = json.NewEncoder(w).Encode(v)
}
