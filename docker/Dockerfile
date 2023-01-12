FROM ubuntu

RUN apt update && \
  apt install -y git curl build-essential gcc-arm-none-eabi gdb-multiarch libclang-dev openocd python3

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

ENV PATH=/root/.cargo/bin:$PATH

RUN mkdir /code
WORKDIR /code/

RUN --mount=type=bind,target=/code/ cargo install cargo-binutils && rustup component add llvm-tools-preview

