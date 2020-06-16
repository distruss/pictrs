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

new_tag=$1

require "$new_tag" "tag"

set -xe

docker manifest create asonix/pictrs:$new_tag \
    -a asonix/pictrs:arm64v8-$new_tag \
    -a asonix/pictrs:arm32v7-$new_tag \
    -a asonix/pictrs:amd64-$new_tag

docker manifest annotate asonix/pictrs:$new_tag \
    asonix/pictrs:arm64v8-$new_tag --os linux --arch arm64 --variant v8

docker manifest annotate asonix/pictrs:$new_tag \
    asonix/pictrs:arm32v7-$new_tag --os linux --arch arm --variant v7

docker manifest annotate asonix/pictrs:$new_tag \
    asonix/pictrs:amd64-$new_tag --os linux --arch amd64

docker manifest push asonix/pictrs:$new_tag --purge
