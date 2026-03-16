FROM debian:bookworm
RUN apt-get update -qq && \
    apt-get install -y -qq live-build debian-archive-keyring >/dev/null 2>&1 && \
    apt-get clean
