FROM rust:1.91-alpine AS builder

RUN apk update && apk add --no-cache musl-dev

WORKDIR /build

COPY . .

RUN cargo build --release


FROM alpine:latest AS runner

COPY --from=builder /build/target/release/apate /

EXPOSE 8228

CMD ["/apate"]
