FROM rustembedded/cross:x86_64-unknown-linux-gnu AS x86_64-builder

ARG UID=1000
ARG GID=1000

RUN \
 apt-get update && \
 apt-get upgrade -y && \
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

ENV \
 PATH=$PATH:/opt/build/.cargo/bin \
 TOOLCHAIN=stable \
 HOST=x86_64-unknown-linux \
 TARGET=x86_64-unknown-linux-gnu \
 TOOL=x86_64-linux-gnu \
 ARCH=amd64

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
    liblzma-dev:$ARCH \
    llvm-dev \
    libclang-dev \
    clang && \
 chown build:build ImageMagick.tar.gz


USER build

RUN \
 tar xvzf ImageMagick.tar.gz && \
 mv ImageMagick-* ImageMagick

WORKDIR /opt/build/ImageMagick

ENV \
 USER=build \
 PKG_CONFIG_ALLOW_CROSS=1 \
 PKG_CONFIG_PATH=/usr/lib/$TOOL/pkgconfig:/usr/lib/pkgconfig \
 LD_LIBRARY_PATH=/usr/lib/$TOOL \
 LD_RUN_PATH=$LD_RUN_PATH:/usr/lib/$TOOL \
 LDFLAGS="$LDFLAGS -L/usr/lib/$TOOL -Wl,-rpath-link,/usr/lib/$TOOL" \
 CFLAGS="$CFLAGS -I/usr/include/$TOOL -I/usr/$TOOL/include -I/usr/include" \
 CPPFLAGS="$CPPFLAGS -I/usr/include/$TOOL -I/usr/$TOOL/include -I/usr/include"

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


USER build

WORKDIR /opt/build

RUN \
 cargo new repo

WORKDIR /opt/build/repo

COPY Cargo.toml Cargo.lock ./

USER root
RUN \
 chown -R build:build ./

USER build

ENV \
 LD_LIBRARY_PATH=$LD_LIBRARY_PATH:/imagemagick/lib \
 LD_RUN_PATH=$LD_RUN_PATH:/imagemagick/lib \
 LDFLAGS="$LDFLAGS -L/imagemagick/lib" \
 RUSTFLAGS="-L/usr/lib/$TOOL -C link-arg=-Wl,-rpath-link,/usr/lib/$TOOL" \
 IMAGE_MAGICK_LIB_DIRS=/imagemagick/lib \
 IMAGE_MAGICK_INCLUDE_DIRS=/imagemagick/include/ImageMagick-7 \
 PKG_CONFIG_PATH=$PKG_CONFIG_PATH:/imagemagick/lib/pkgconfig \
 CFLAGS="$CFLAGS -I/imagemagick/include/ImageMagick-7" \
 CPPFLAGS="$CPPFLAGS -I/imagemagick/include/ImageMagick-7"

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

RUN cargo build --frozen

FROM ubuntu:20.04

ARG UID=1000
ARG GID=1000
ARG BINARY=pict-rs
ARG TARGET=x86_64-unknown-linux-gnu

RUN \
 apt-get update && \
 apt-get -y upgrade && \
 apt-get -y install \
    tini \
    libgexiv2-2 \
    libgomp1 \
    libltdl7 && \
 addgroup --gid $GID pictrs && \
 adduser \
    --disabled-password \
    --gecos "" \
    --ingroup pictrs \
    --uid $UID \
    --home /opt/pictrs \
    pictrs

COPY --from=builder /imagemagick /imagemagick
COPY --from=builder /opt/build/repo/target/debug/$BINARY /usr/bin/$BINARY

VOLUME /mnt
WORKDIR /opt/pictrs
USER pictrs
EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/bin/pict-rs", "-p", "/mnt", "-a", "0.0.0.0:8080"]
