package config

import "os"

type Config struct {
        Addr        string
        DatabaseURL string
}

func Load() Config {
        addr := os.Getenv("STROEX_API_ADDR")
        if addr == "" {
                addr = ":8080"
        }
        dbURL := os.Getenv("DATABASE_URL")
        if dbURL == "" {
                dbURL = "postgres://stroex:stroex@localhost:5432/stroex?sslmode=disable"
        }
        return Config{Addr: addr, DatabaseURL: dbURL}
}
