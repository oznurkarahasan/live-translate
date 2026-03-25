FROM rust:1.77-slim-bookworm

RUN apt-get update && apt-get install -y \
    pkg-config \
    libasound2-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app/backend

# RUN cargo install cargo-watch

COPY . .

CMD ["cargo", "run"]