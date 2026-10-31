"""Test-only TLS terminator for the release smoke test. Never shipped.

    python3 tls-proxy.py CERT KEY LISTEN_PORT UPSTREAM_PORT

Accepts TLS on 127.0.0.1:LISTEN_PORT with CERT/KEY and forwards plain TCP to
127.0.0.1:UPSTREAM_PORT, so the CLI can be tested against a private-CA HTTPS server.
"""

import asyncio
import ssl
import sys


async def pump(reader, writer):
    try:
        while data := await reader.read(65536):
            writer.write(data)
            await writer.drain()
    except (ConnectionError, ssl.SSLError):
        pass
    finally:
        writer.close()


async def main(cert, key, listen, upstream):
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)

    async def handle(client_reader, client_writer):
        up_reader, up_writer = await asyncio.open_connection("127.0.0.1", upstream)
        await asyncio.gather(pump(client_reader, up_writer), pump(up_reader, client_writer))

    server = await asyncio.start_server(handle, "127.0.0.1", listen, ssl=context)
    print("ready", flush=True)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])))
