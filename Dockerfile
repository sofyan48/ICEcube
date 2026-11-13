# --- frontend (Bun) ---
FROM oven/bun:1.4 AS frontend
WORKDIR /app
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile
COPY frontend/ ./
RUN bun run build

# --- backend (Rust) ---
FROM rust:1.99-slim AS backend
WORKDIR /app
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY backend/src ./src
RUN touch src/main.rs && cargo build --release

# --- runtime ---
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=backend /app/target/release/icecube /app/icecube
COPY --from=frontend /app/dist /app/static
ENV ICECUBE_STATIC_DIR=/app/static ICECUBE_LISTEN=0.0.0.0:4000
EXPOSE 4000
ENTRYPOINT ["/app/icecube"]
