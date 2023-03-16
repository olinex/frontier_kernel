# FrontierOS

## 1 DEV

### 1.1 Environment Prepare

Only tested on ubuntu Linux version 5.15.49-linuxkit

#### 1.1.1 Install Rust

```bash
rustup target add riscv64gc-unknown-none-elf
cargo install cargo-binutils
rustup component add llvm-tools-preview
rustup component add rust-src
```

#### 1.1.2 Install qemu

* Install compile dependencies for eqmu

  ```bash
  sudo apt install autoconf automake autotools-dev curl libmpc-dev libmpfr-dev libgmp-dev \
    gawk build-essential bison flex texinfo gperf libtool patchutils bc \
    zlib1g-dev libexpat-dev pkg-config  libglib2.0-dev libpixman-1-dev libsdl2-dev \
    git tmux python3 python3-pip ninja-build
  ```

* Download and unzip qemu
  
  ```bash
  cd /mnt/ \
  && wget https://download.qemu.org/qemu-7.0.0.tar.xz \
  && tar xvJf qemu-7.0.0.tar.xz
  ```

* Compile qemu and export as executable command

  ```bash
  cd /mnt/qemu-7.0.0 \
  && ./configure --target-list=riscv64-softmmu,riscv64-linux-user \
  && make -j$(nproc) \
  && echo "export PATH=\$PATH:/mnt/qemu-7.0.0/build" >> ~/.bashrc \
  && source ~/.bashrc
  ```

#### 1.1.3 Install gdb

#### 1.1.4 Install gdb dashboard (Optional)
