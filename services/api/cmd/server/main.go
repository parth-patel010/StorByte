package main

import (
        "context"
        "log"
        "net/http"
        "os"
        "os/signal"
        "syscall"
        "time"

        "github.com/joho/godotenv"
        "github.com/stroex/api/internal/config"
        "github.com/stroex/api/internal/db"
        httpapi "github.com/stroex/api/internal/http"
)

func main() {
        _ = godotenv.Load()
        cfg := config.Load()

        ctx := context.Background()
        pool, err := db.Connect(ctx, cfg.DatabaseURL)
        if err != nil {
                log.Printf("database unavailable (%v); starting with degraded /health", err)
                pool = nil
        } else {
                defer pool.Close()
                if err := db.Migrate(ctx, pool); err != nil {
                        log.Printf("migrate warning: %v", err)
                }
        }

        srv := &http.Server{
                Addr:              cfg.Addr,
                Handler:           httpapi.NewRouter(pool),
                ReadHeaderTimeout: 5 * time.Second,
        }

        go func() {
                log.Printf("Stroex API listening on %s", cfg.Addr)
                if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
                        log.Fatalf("serve: %v", err)
                }
        }()

        stop := make(chan os.Signal, 1)
        signal.Notify(stop, syscall.SIGINT, syscall.SIGTERM)
        <-stop

        shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
        defer cancel()
        _ = srv.Shutdown(shutdownCtx)
}
