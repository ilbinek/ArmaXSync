# Multi-arch server image (linux/amd64, linux/arm64).
# Build: docker buildx build -f packaging/docker/Dockerfile --platform linux/amd64,linux/arm64 -t ghcr.io/ilbinek/armaxsync-server .
FROM node:22-alpine AS ui
WORKDIR /src/ui
RUN corepack enable
COPY ui/ ./
RUN pnpm install --frozen-lockfile=false && pnpm --filter admin build

FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
COPY --from=ui /src/ui/apps/admin/build ui/apps/admin/build
RUN cargo build --release -p armaxsync-server && strip target/release/armaxsync-server

FROM alpine:3
RUN apk add --no-cache ca-certificates tzdata && adduser -D -u 1000 armaxsync
COPY --from=build /src/target/release/armaxsync-server /usr/local/bin/armaxsync-server
USER armaxsync
ENV ARMAXSYNC_DATA_DIR=/data ARMAXSYNC_BIND=0.0.0.0:8080 ARMAXSYNC_INSTALL_KIND=docker
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s CMD ["armaxsync-server", "healthcheck"]
ENTRYPOINT ["armaxsync-server"]
CMD ["serve"]
