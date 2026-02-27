# Stage 1: Build
FROM rust:latest AS builder
WORKDIR /app
COPY . .
# Build for release
RUN cargo build --release

# Stage 2: Runtime
FROM debian:stable
WORKDIR /app
# Copy only the compiled binary from the builder stage
COPY --from=builder /app/target/release/AsistentVirtual-backend ./backend

EXPOSE 8000
CMD ["./backend"]
