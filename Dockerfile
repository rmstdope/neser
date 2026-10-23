FROM caddy:2.10.2-alpine
ARG REVISION
COPY deploy/Caddyfile /etc/caddy/Caddyfile
COPY dist/index.html /srv/index.html
COPY dist/assets/ /srv/assets/
RUN test -n "$REVISION" \
    && printf '{"revision":"%s"}\n' "$REVISION" > /srv/release.json \
    && mkdir /srv/roms \
    && printf '{"roms":[]}\n' > /srv/roms/roms.json
EXPOSE 8080
HEALTHCHECK --interval=5s --timeout=3s --retries=12 CMD wget -q --spider http://127.0.0.1:8080/healthz || exit 1
