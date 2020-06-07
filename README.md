# pict-rs
_a simple image hosting service_

## Usage
### Running
```
$ ./pict-rs --help
pict-rs 0.1.0

USAGE:
    pict-rs [OPTIONS] --addr <addr> --path <path>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -a, --addr <addr>        The address and port the server binds to, e.g. 127.0.0.1:80
    -f, --format <format>    An image format to convert all uploaded files into, supports 'jpg' and 'png'
    -p, --path <path>        The path to the data directory, e.g. data/
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
- `GET /image/download?url=...` Download an image from a remote server, returning the same JSON
    payload as the `POST` endpoint
- `GET /image/{file}` for getting a full-resolution image. `file` here is the `file` key from the
    `/image` endpoint's JSON
- `GET /image/{size}/{file}` where `size` is a positive integer. This endpoint is for accessing
    thumbnails from an uploaded image.
- `DELETE /image/{delete_token}/{file}` to delete a file, where `delete_token` and `file` are from
    the `/image` endpoint's JSON

## Contributing
Feel free to open issues for anything you find an issue with. Please note that any contributed code will be licensed under the AGPLv3.

## License

Copyright © 2020 Riley Trautman

pict-rs is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

pict-rs is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details. This file is part of pict-rs.

You should have received a copy of the GNU General Public License along with pict-rs. If not, see [http://www.gnu.org/licenses/](http://www.gnu.org/licenses/).
