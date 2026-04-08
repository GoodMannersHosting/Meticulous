# Meticulous web UI (SvelteKit SSR, adapter-node). Node 24; builder uses trixie slim to match distroless nodejs24 (Debian 13 glibc).
#
# PUBLIC_API_URL is baked into the client at build time ($env/static/public).
# Set ORIGIN at run time to the browser-facing origin (e.g. https://app.example.com) when behind a reverse proxy.
#
#   docker build -f Dockerfile \
#     --build-arg PUBLIC_API_URL=https://api.example.com \
#     -t meticulous/frontend:latest .
#
#   docker run --rm -p 3000:3000 \
#     -e ORIGIN=http://127.0.0.1:3000 \
#     meticulous/frontend:latest
#
# Podman: same flags (podman build / podman run).

FROM node:24-trixie-slim AS builder
WORKDIR /app

RUN apt-get update \
	&& apt-get install -y --no-install-recommends ca-certificates \
	&& rm -rf /var/lib/apt/lists/*

COPY package.json package-lock.json ./
RUN npm ci

COPY . .

# Empty default: browser uses the page origin (same hostname as the API behind a path-based gateway).
# Local dev: set PUBLIC_API_URL in `.env` or pass --build-arg PUBLIC_API_URL=http://localhost:8080.
ARG PUBLIC_API_URL=
ENV PUBLIC_API_URL=${PUBLIC_API_URL}

RUN npm run build \
	&& npm prune --omit=dev

FROM gcr.io/distroless/nodejs24-debian13

WORKDIR /app

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=3000

COPY --from=builder --chown=nonroot:nonroot /app/build ./build
COPY --from=builder --chown=nonroot:nonroot /app/package.json ./
COPY --from=builder --chown=nonroot:nonroot /app/node_modules ./node_modules

USER nonroot:nonroot

CMD ["build/index.js"]
