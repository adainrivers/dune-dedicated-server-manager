ARG UBUNTU_VERSION=26.04
ARG NODE_MAJOR=22
ARG BUILD_UID=1000
ARG BUILD_GID=1000

FROM ubuntu:${UBUNTU_VERSION}

ARG NODE_MAJOR
ARG BUILD_UID
ARG BUILD_GID

ENV DEBIAN_FRONTEND=noninteractive
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:${PATH}

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        wget \
        file \
        git \
        build-essential \
        pkg-config \
        libwebkit2gtk-4.1-dev \
        libxdo-dev \
        libssl-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        xz-utils \
    && install -d -m 0755 /etc/apt/keyrings \
    && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
        -o /etc/apt/keyrings/nodesource.asc \
    && echo "deb [signed-by=/etc/apt/keyrings/nodesource.asc] https://deb.nodesource.com/node_${NODE_MAJOR}.x nodistro main" \
        > /etc/apt/sources.list.d/nodesource.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends nodejs \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustup --version \
    && cargo --version \
    && node --version \
    && npm --version \
    && group_name="$(getent group "${BUILD_GID}" | cut -d: -f1)" \
    && if [ -z "${group_name}" ]; then groupadd --gid "${BUILD_GID}" builder && group_name=builder; fi \
    && user_name="$(getent passwd "${BUILD_UID}" | cut -d: -f1)" \
    && if [ -z "${user_name}" ]; then useradd --uid "${BUILD_UID}" --gid "${BUILD_GID}" --create-home builder && user_name=builder; fi \
    && chown -R "${user_name}:${group_name}" /usr/local/rustup /usr/local/cargo \
    && printf '%s\n' "${user_name}" > /tmp/build-user \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

USER ${BUILD_UID}:${BUILD_GID}
WORKDIR /workspace

CMD ["bash", "-lc", "cd app && npm ci && npm run tauri -- build --bundles appimage"]
