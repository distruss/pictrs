#!/usr/bin/env bash

export USER_ID=$(id -u)
export GROUP_ID=$(id -g)

mkdir -p ./volumes/pictrs

docker-compose up --build
