#!/usr/bin/env python3

# This script could be used for actix-web multipart example test
# just start server and run client.py

import asyncio
import aiofiles
import aiohttp

file_name = '../test.png'
url = 'http://localhost:8080/image'

async def file_sender(file_name=None):
    async with aiofiles.open(file_name, 'rb') as f:
        chunk = await f.read(64*1024)
        while chunk:
            yield chunk
            chunk = await f.read(64*1024)


async def req():
    async with aiohttp.ClientSession() as session:
        data = aiohttp.FormData(quote_fields=False)
        data.add_field("images[]", file_sender(file_name=file_name), filename="image1.png", content_type="image/png")
        data.add_field("images[]", file_sender(file_name=file_name), filename="image2.png", content_type="image/png")
        data.add_field("images[]", file_sender(file_name=file_name), filename="image3.png", content_type="image/png")

        async with session.post(url, data=data) as resp:
            text = await resp.text()
            print(text)
            assert 201 == resp.status


loop = asyncio.get_event_loop()
loop.run_until_complete(req())
