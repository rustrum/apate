FROM alpine:latest AS downloader

# TARGETPLATFORM is automatically injected by Docker Buildx (linux/amd64 or linux/arm64)
ARG TARGETPLATFORM
ARG GH_TAG

RUN apk add --no-cache curl ca-certificates

RUN if [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
        ARCH="aarch64-unknown-linux-musl"; \
    else \
        ARCH="x86_64-unknown-linux-musl"; \
    fi && \
    echo "Downloading apate ${GH_TAG} for ${ARCH}..." && \
    curl -sSL "https://github.com/rustrum/apate/releases/download/${GH_TAG}/apate-${ARCH}.tar.gz" | tar -xz -C /bin/


FROM alpine:latest AS runner

RUN apk add --no-cache ca-certificates

COPY --from=downloader /bin/apate /bin/

EXPOSE 8228

ENV RUST_LOG=info

ENTRYPOINT ["/bin/apate"]
