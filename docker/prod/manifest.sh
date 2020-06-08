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
    echo "	manifest.sh [tag]"
    echo ""
    echo "Args:"
    echo "	tag: The git tag to be applied to the image manifest"
}

function annotate() {
    tag=$1
    arch=$2

    docker manifest annotate asonix/pictrs:$tag \
        asonix/pictrs:$arch-$tag --os linux --arch $arch
}

new_tag=$1

require "$new_tag" "tag"

set -xe

docker manifest create asonix/pictrs:$new_tag \
    asonix/pictrs:arm64v8-$new_tag \
    asonix/pictrs:arm32v7-$new_tag \
    asonix/pictrs:amd64-$new_tag

annotate $new_tag arm64v8
annotate $new_tag arm32v7
annotate $new_tag amd64

# docker manifest push asonix/pictrs:$new_tag
