#!/usr/bin/env bash

function require() {
    if [ "$1" = "" ]; then
        echo "input '$2' required"
        print_help
        exit 1
    fi
}

function print_help() {
    echo "deploy.sh"
    echo ""
    echo "Usage:"
    echo "	deploy.sh [tag]"
    echo ""
    echo "Args:"
    echo "	tag: The git tag to be applied to the repository and docker build"
}

function build_image() {
    tag=$1
    arch=$2

    ./build-image.sh asonix/pictrs $tag $arch

    docker push asonix/pictrs:$arch-$tag
    docker push asonix/pictrs:$arch-latest
}

# Creating the new tag
new_tag="$1"

require "$new_tag" "tag"

if ! docker run --rm -it arm64v8/alpine:3.11 /bin/sh -c 'echo "docker is configured correctly"'
then
    echo "docker is not configured to run on qemu-emulated architectures, fixing will require sudo"
    sudo docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
fi

set -xe

git checkout master

# Changing the docker-compose prod
sed -i "s/asonix\/pictrs:.*/asonix\/pictrs:$new_tag/" docker-compose.yml
git add ../prod/docker-compose.yml

# The commit
git commit -m"Version $new_tag"
git tag $new_tag

# Push
git push origin $new_tag
git push

# Build for arm64v8, arm32v7, and amd64
build_image $new_tag arm64v8
build_image $new_tag arm32v7
build_image $new_tag amd64

# Build for other archs
# TODO

./manifest.sh $new_tag
./manifest.sh latest
