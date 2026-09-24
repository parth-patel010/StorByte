package db

import (
        "context"
        "fmt"
        "os"
        "path/filepath"
        "sort"
        "strings"

        "github.com/jackc/pgx/v5/pgxpool"
)

func Connect(ctx context.Context, url string) (*pgxpool.Pool, error) {
        pool, err := pgxpool.New(ctx, url)
        if err != nil {
                return nil, err
        }
        if err := pool.Ping(ctx); err != nil {
                pool.Close()
                return nil, err
        }
        return pool, nil
}

func Migrate(ctx context.Context, pool *pgxpool.Pool) error {
        _, err := pool.Exec(ctx, `
                CREATE TABLE IF NOT EXISTS schema_migrations (
                        filename TEXT PRIMARY KEY,
                        applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                )`)
        if err != nil {
                return err
        }

        dir := migrationsDir()
        entries, err := os.ReadDir(dir)
        if err != nil {
                return fmt.Errorf("read migrations at %s: %w", dir, err)
        }
        var names []string
        for _, e := range entries {
                if !e.IsDir() && strings.HasSuffix(e.Name(), ".sql") {
                        names = append(names, e.Name())
                }
        }
        sort.Strings(names)

        for _, name := range names {
                var exists bool
                if err := pool.QueryRow(ctx, `SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE filename=$1)`, name).Scan(&exists); err != nil {
                        return err
                }
                if exists {
                        continue
                }
                body, err := os.ReadFile(filepath.Join(dir, name))
                if err != nil {
                        return err
                }
                tx, err := pool.Begin(ctx)
                if err != nil {
                        return err
                }
                if _, err := tx.Exec(ctx, string(body)); err != nil {
                        _ = tx.Rollback(ctx)
                        return fmt.Errorf("migration %s: %w", name, err)
                }
                if _, err := tx.Exec(ctx, `INSERT INTO schema_migrations(filename) VALUES ($1)`, name); err != nil {
                        _ = tx.Rollback(ctx)
                        return err
                }
                if err := tx.Commit(ctx); err != nil {
                        return err
                }
        }
        return nil
}

func migrationsDir() string {
        if v := os.Getenv("STROEX_MIGRATIONS_DIR"); v != "" {
                return v
        }
        candidates := []string{
                "migrations",
                filepath.Join("services", "api", "migrations"),
                filepath.Join("..", "migrations"),
                filepath.Join("..", "..", "migrations"),
        }
        for _, c := range candidates {
                if st, err := os.Stat(c); err == nil && st.IsDir() {
                        return c
                }
        }
        return "migrations"
}
