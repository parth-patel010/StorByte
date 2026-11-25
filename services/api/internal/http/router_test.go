package httpapi

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

type pingFunc func(context.Context) error

func (f pingFunc) Ping(ctx context.Context) error { return f(ctx) }

func TestHealth(t *testing.T) {
	for _, tt := range []struct {
		name      string
		withPool  bool
		pingError error
		cancelled bool
		status    string
		database  bool
	}{
		{name: "nil pool", status: "degraded"},
		{name: "healthy pool", withPool: true, status: "ok", database: true},
		{name: "failed ping", withPool: true, pingError: errors.New("database unavailable"), status: "degraded"},
		{name: "cancelled request", withPool: true, pingError: context.Canceled, cancelled: true, status: "degraded"},
	} {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if tt.cancelled {
				cancel()
			}
			req := httptest.NewRequest(http.MethodGet, "/health", nil).WithContext(ctx)
			handler := NewRouter(nil)
			pings := 0
			if tt.withPool {
				r := &Router{mux: http.NewServeMux(), pool: pingFunc(func(got context.Context) error {
					pings++
					if got != req.Context() {
						t.Error("database ping did not receive the request context")
					}
					if tt.cancelled && got.Err() != context.Canceled {
						t.Error("request cancellation was not propagated")
					}
					return tt.pingError
				})}
				r.routes()
				handler = r.mux
			}

			before := time.Now().UTC().Truncate(time.Second)
			rr := httptest.NewRecorder()
			handler.ServeHTTP(rr, req)
			after := time.Now().UTC()
			if rr.Code != http.StatusOK {
				t.Fatalf("status = %d, want 200", rr.Code)
			}
			if got := rr.Header().Get("Content-Type"); got != "application/json" {
				t.Errorf("Content-Type = %q, want application/json", got)
			}
			var body struct {
				Status    string `json:"status"`
				Service   string `json:"service"`
				Time      string `json:"time"`
				Database  *bool  `json:"database"`
				FileProxy *bool  `json:"fileProxy"`
			}
			if err := json.Unmarshal(rr.Body.Bytes(), &body); err != nil {
				t.Fatalf("decode health JSON: %v", err)
			}
			if body.Status != tt.status || body.Service != "storbyte-api" {
				t.Errorf("status/service = %q/%q, want %q/storbyte-api", body.Status, body.Service, tt.status)
			}
			if body.Database == nil || *body.Database != tt.database {
				t.Errorf("database = %v, want present and %t", body.Database, tt.database)
			}
			if body.FileProxy == nil || *body.FileProxy {
				t.Error("fileProxy must be present and false")
			}
			stamp, err := time.Parse(time.RFC3339, body.Time)
			if err != nil {
				t.Fatalf("invalid health timestamp %q: %v", body.Time, err)
			}
			if stamp.Location() != time.UTC || stamp.Before(before) || stamp.After(after) {
				t.Errorf("timestamp %q must be UTC and within the request window", body.Time)
			}
			if tt.withPool && pings != 1 {
				t.Errorf("database ping calls = %d, want 1", pings)
			}
		})
	}
}

func TestPhaseOneStubRoutes(t *testing.T) {
	handler := NewRouter(nil)
	for _, tt := range []struct{ method, path string }{
		{http.MethodPost, "/auth/register"},
		{http.MethodPost, "/auth/login"},
		{http.MethodGet, "/user"},
		{http.MethodGet, "/devices"},
		{http.MethodPost, "/devices"},
		{http.MethodDelete, "/devices/test-device"},
		{http.MethodGet, "/license"},
		{http.MethodPost, "/license/activate"},
	} {
		t.Run(tt.method+" "+tt.path, func(t *testing.T) {
			rr := httptest.NewRecorder()
			handler.ServeHTTP(rr, httptest.NewRequest(tt.method, tt.path, nil))
			if rr.Code != http.StatusNotImplemented {
				t.Fatalf("status = %d, want 501", rr.Code)
			}
			if got := rr.Header().Get("Content-Type"); got != "application/json" {
				t.Errorf("Content-Type = %q, want application/json", got)
			}
			var body map[string]string
			if err := json.Unmarshal(rr.Body.Bytes(), &body); err != nil {
				t.Fatalf("decode stub JSON: %v", err)
			}
			if len(body) != 1 || body["error"] != "not implemented in Phase 1 prototype scaffold" {
				t.Errorf("unexpected stub error shape: %v", body)
			}
		})
	}
}

func TestRouterDispatch(t *testing.T) {
	handler := NewRouter(nil)
	for _, tt := range []struct {
		method, path string
		status       int
	}{
		{http.MethodPost, "/health", http.StatusMethodNotAllowed},
		{http.MethodGet, "/auth/register", http.StatusMethodNotAllowed},
		{http.MethodPut, "/devices/test-device", http.StatusMethodNotAllowed},
		{http.MethodGet, "/not-a-route", http.StatusNotFound},
	} {
		t.Run(tt.method+" "+tt.path, func(t *testing.T) {
			rr := httptest.NewRecorder()
			handler.ServeHTTP(rr, httptest.NewRequest(tt.method, tt.path, nil))
			if rr.Code != tt.status {
				t.Errorf("status = %d, want %d", rr.Code, tt.status)
			}
		})
	}
}
