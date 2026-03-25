FROM rust:1.87-slim-bookworm

RUN apt-get update && apt-get install -y \
    pkg-config \
    libasound2-dev \
    libasound2-plugins \
    libpulse-dev \
    libpulse0 \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Route the ALSA "default" device through PulseAudio.
# cpal uses ALSA; this bridge makes it transparently use the host's PulseAudio
# daemon (whose socket is mounted in via docker-compose).
RUN echo 'pcm.default pulse\nctl.default pulse' > /etc/asound.conf

WORKDIR /app/backend

# RUN cargo install cargo-watch

COPY . .

CMD ["cargo", "run"]