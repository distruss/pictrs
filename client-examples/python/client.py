#!/usr/bin/env python3

# This script could be used for actix-web multipart example test
# just start server and run client.py

import asyncio
import aiofiles
import aiohttp

png_name = '../test.png'
gif_name = '../earth.gif'
jpeg_name = '../cat.jpg'
webp_name = '../scene.webp'
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
        data.add_field("images[]", file_sender(file_name=png_name), filename="image1.png", content_type="image/png")
        data.add_field("images[]", file_sender(file_name=png_name), filename="image2.png", content_type="image/png")
        data.add_field("images[]", file_sender(file_name=gif_name), filename="image1.gif", content_type="image/gif")
        data.add_field("images[]", file_sender(file_name=gif_name), filename="image2.gif", content_type="image/gif")
        data.add_field("images[]", file_sender(file_name=jpeg_name), filename="image1.jpeg", content_type="image/jpeg")
        data.add_field("images[]", file_sender(file_name=jpeg_name), filename="image2.jpeg", content_type="image/jpeg")
        data.add_field("images[]", file_sender(file_name=webp_name), filename="image1.webp", content_type="image/webp")
        data.add_field("images[]", file_sender(file_name=webp_name), filename="image2.webp", content_type="image/webp")

        async with session.post(url, data=data) as resp:
            text = await resp.text()
            print(text)
            assert 201 == resp.status


loop = asyncio.get_event_loop()
loop.run_until_complete(req())
