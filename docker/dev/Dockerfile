FROM rustembedded/cross:x86_64-unknown-linux-gnu AS x86_64-builder

ARG UID=1000
ARG GID=1000

ENV TOOLCHAIN=stable
ENV HOST=x86_64-unknown-linux
ENV TARGET=$HOST-gnu
ENV TOOL=x86_64-linux-gnu
ENV ARCH=amd64

RUN \
 apt-get update && \
 apt-get upgrade -y

RUN \
 addgroup --gid "${GID}" build && \
 adduser \
    --disabled-password \
    --gecos "" \
    --ingroup build \
    --uid "${UID}" \
    --home /opt/build \
    build

ADD https://sh.rustup.rs /opt/build/rustup.sh

RUN \
 chown -R build:build /opt/build

USER build
WORKDIR /opt/build

ENV PATH=$PATH:/opt/build/.cargo/bin

RUN \
 chmod +x rustup.sh && \
 ./rustup.sh --default-toolchain $TOOLCHAIN --profile minimal -y && \
 rustup target add $TARGET

FROM x86_64-builder as builder

USER root

ADD https://imagemagick.org/download/ImageMagick.tar.gz ./

RUN \
 dpkg --add-architecture $ARCH && \
 apt-get update && \
 apt-get -y install \
    libgexiv2-dev:$ARCH \
    libltdl-dev:$ARCH \
    libjpeg-dev:$ARCH \
    libpng-dev:$ARCH \
    libwebp-dev:$ARCH \
    libxml2-dev:$ARCH && \
 chown build:build ImageMagick.tar.gz


USER build
ENV USER=build

RUN \
 tar xvzf ImageMagick.tar.gz && \
 mv ImageMagick-* ImageMagick

WORKDIR /opt/build/ImageMagick

RUN \
 ./configure \
     CC=$TOOL-gcc \
     CXX=$TOOL-g++ \
     --prefix=/imagemagick \
     --with-modules \
     --enable-shared \
     --disable-static \
     --without-perl \
     --with-xml=yes \
     --with-png=yes \
     --with-jpeg=yes \
     --with-webp=yes \
     --host=$HOST && \
 make

USER root

RUN \
 make install && \
 ldconfig /imagemagick/lib

ENV LD_LIBRARY_PATH=$LD_LIBRARY_PATH:/imagemagick/lib
ENV LD_RUN_PATH=$LD_RUN_PATH:/imagemagick/lib
ENV LDFLAGS="$LDFLAGS -L/imagemagick/lib"
ENV RUSTFLAGS="-L/usr/lib/$TOOL -C link-arg=-Wl,-rpath-link,/usr/lib/$TOOL"
ENV IMAGE_MAGICK_LIB_DIRS=/imagemagick/lib
ENV IMAGE_MAGICK_INCLUDE_DIRS=/imagemagick/include/ImageMagick-7
ENV PKG_CONFIG_PATH=$PKG_CONFIG_PATH:/usr/lib/$TOOL/pkgconfig:/usr/lib/$PKGCONFIG:/imagemagick/lib/pkgconfig
ENV CPPFLAGS="$CPPFLAGS -I/imagemagick/include/ImageMagick-7 -I/usr/lib/llvm-6.0/lib/clang/6.0.0/include"

USER build

WORKDIR /opt/build

# Cache deps
RUN \
 cargo new repo

WORKDIR /opt/build/repo

COPY Cargo.toml Cargo.lock ./

USER root
RUN \
 chown -R build:build ./ && \
 apt-get install -y \
    llvm-dev:$ARCH \
    libclang-dev:$ARCH \
    clang:$ARCH

USER build

RUN \
 ls /imagemagick/* && \
 ls /imagemagick/*/*

RUN \
 mkdir -p ./src && \
 echo 'fn main() { println!("Dummy") }' > ./src/main.rs && \
 cargo build && \
 rm -rf ./src

COPY src ./src/

USER root
RUN \
 chown -R build:build ./src && \
 rm -r ./target/debug/deps/pict_rs-*

USER build

# Build
RUN cargo build --frozen

FROM ubuntu:20.04

ARG UID=1000
ARG GID=1000

RUN apt-get update \
  && apt-get install -y \
      libgexiv2-2 \
      libgomp1 \
      libltdl7


# Copy resources
COPY --from=builder /opt/build/repo/target/debug/pict-rs /usr/bin/pict-rs
COPY --from=builder /imagemagick /imagemagick

RUN \
 addgroup -gid "${GID}" pictrs && \
 adduser \
    --disabled-password \
    --gecos "" \
    --ingroup pictrs \
    --uid "${UID}" \
    --home /opt/pictrs \
    pictrs
WORKDIR /opt/pictrs
USER pictrs
EXPOSE 8080
CMD ["/usr/bin/pict-rs"]
