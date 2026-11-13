# --- frontend (Bun) ---
FROM oven/bun:1.4 AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile
COPY frontend/ ./
RUN bun run build

# --- backend (Rust); the built UI is embedded into the binary ---
FROM rust:1.99-slim AS backend
WORKDIR /app/backend
# Cache dependencies separately from the app code.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && ICECUBE_ALLOW_NO_UI=1 cargo build --release --locked \
    && rm -rf src
COPY backend/build.rs ./
COPY backend/src ./src
COPY --from=frontend /app/frontend/dist /app/frontend/dist
RUN touch src/main.rs build.rs && cargo build --release --locked

# --- runtime: a single binary ---
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=backend /app/backend/target/release/icecube /icecube
ENV ICECUBE_LISTEN=0.0.0.0:4000
EXPOSE 4000
ENTRYPOINT ["/icecube"]
