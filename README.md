# pict-rs
_a simple image hosting service_

## Usage
### Running
```
pict-rs 0.1.4

USAGE:
    pict-rs [FLAGS] [OPTIONS] --path <path>

FLAGS:
    -h, --help                     Prints help information
    -s, --skip-validate-imports    Whether to skip validating images uploaded via the internal import API
    -V, --version                  Prints version information

OPTIONS:
    -a, --addr <addr>                      The address and port the server binds to. Default: 0.0.0.0:8080 [env:
                                           PICTRS_ADDR=]  [default: 0.0.0.0:8080]
    -f, --format <format>                  An optional image format to convert all uploaded files into, supports 'jpg'
                                           and 'png' [env: PICTRS_FORMAT=]
    -m, --max-file-size <max-file-size>    Specify the maximum allowed uploaded file size (in Megabytes) [env:
                                           PICTRS_MAX_FILE_SIZE=]  [default: 40]
    -p, --path <path>                      The path to the data directory, e.g. data/ [env: PICTRS_PATH=]
    -w, --whitelist <whitelist>...         An optional list of filters to whitelist, supports 'identity', 'thumbnail',
                                           and 'blur' [env: PICTRS_FILTER_WHITELIST=]
```

#### Example:
Running on all interfaces, port 8080, storing data in /opt/data
```
$ ./pict-rs -a 0.0.0.0:8080 -p /opt/data
```
Running locally, port 9000, storing data in data/, and converting all uploads to PNG
```
$ ./pict-rs -a 127.0.0.1:9000 -p data/ -f png
```
Running locally, port 8080, storing data in data/, and only allowing the `thumbnail` and `identity` filters
```
$ ./pict-rs -a 127.0.0.1:8080 -p data/ -w thumbnail identity
```

#### Docker
Run the following commands:
```
# Create a folder for the files (anywhere works)
mkdir ./pict-rs
cd ./pict-rs
mkdir -p volumes/pictrs
sudo chown -R 991:991 volumes/pictrs
wget https://git.asonix.dog/asonix/pict-rs/raw/branch/master/docker/prod/docker-compose.yml
sudo docker-compose up -d
```

#### Docker Development
Run the following to develop in docker:
```
git clone https://git.asonix.dog/asonix/pict-rs
cd pict-rs/docker/dev
docker-compose up --build
```

### API
pict-rs offers four endpoints:
- `POST /image` for uploading an image. Uploaded content must be valid multipart/form-data with an
    image array located within the `images[]` key

    This endpoint returns the following JSON structure on success with a 201 Created status
    ```json
    {
        "files": [
            {
                "delete_token": "JFvFhqJA98",
                "file": "lkWZDRvugm.jpg"
            },
            {
                "delete_token": "kAYy9nk2WK",
                "file": "8qFS0QooAn.jpg"
            },
            {
                "delete_token": "OxRpM3sf0Y",
                "file": "1hJaYfGE01.jpg"
            }
        ],
        "msg": "ok"
    }
    ```
- `POST /import` for uploading an image while preserving the filename. This should not be exposed to
    the public internet, as it can cause naming conflicts with saved files. The upload format and
    response format are the same as the `POST /image` endpoint.
- `GET /image/download?url=...` Download an image from a remote server, returning the same JSON
    payload as the `POST` endpoint
- `GET /image/{file}` for getting a full-resolution image. `file` here is the `file` key from the
    `/image` endpoint's JSON
- `GET /image/{transformations...}/{file}` get a file with transformations applied.
    existing transformations include
    - `identity`: apply no changes
    - `blur{float}`: apply a gaussian blur to the file
    - `thumbnail{int}`: produce a thumbnail of the image fitting inside an `{int}` by `{int}` square
    An example of usage could be
    ```
    GET /image/thumbnail256/blur3.0/asdf.png
    ```
    which would create a 256x256px
    thumbnail and blur it
- `DELETE /image/delete/{delete_token}/{file}` or `GET /image/delete/{delete_token}/{file}` to delete a file,
    where `delete_token` and `file` are from the `/image` endpoint's JSON

## Contributing
Feel free to open issues for anything you find an issue with. Please note that any contributed code will be licensed under the AGPLv3.

## License

Copyright © 2020 Riley Trautman

pict-rs is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

pict-rs is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details. This file is part of pict-rs.

You should have received a copy of the GNU General Public License along with pict-rs. If not, see [http://www.gnu.org/licenses/](http://www.gnu.org/licenses/).
