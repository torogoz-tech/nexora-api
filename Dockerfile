# syntax=docker/dockerfile:1.7

FROM api-artifact AS compiled-api

FROM ubuntu:24.04 AS runtime

RUN groupadd --system nexora \
    && useradd --system --gid nexora --no-create-home nexora \
    && apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl git libgcc-s1 \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o nexora -g nexora /var/lib/nexora

COPY --from=compiled-api /nexora-api /usr/local/bin/nexora-api

USER nexora
WORKDIR /var/lib/nexora
EXPOSE 18080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl --fail --silent http://127.0.0.1:18080/health || exit 1

ENTRYPOINT ["/usr/local/bin/nexora-api"]
CMD ["serve", "--bind", "0.0.0.0:18080"]
