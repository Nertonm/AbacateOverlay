import asyncio
import struct   
import os
import logging

HOST = os.getenv("HOST", "127.0.0.1")
PORT = int(os.getenv("PORT", 42069))
logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
logging = logging.getLogger("ImageServer")

async def handle_client(reader, writer):
    address = writer.get_extra_info('peername')
    logging.info(f"Connection from {address}")

    try:
        while True:
            # Image Size, Read the 4-byte header
            header = await reader.readexactly(4)
            if not header:
                break

            # Unpack the image size, big-endian unsigned int
            # ! means network (= big-endian), I means unsigned int (4 bytes)
            # Returns a tuple, so we unpack it directly
            (img_size,) = struct.unpack('!I', header)

            logging.info(f"Expecting image of size: {img_size} bytes")

            # Read the image data based on the size
            img_data = await reader.readexactly(img_size)

            # TODO: Send to MangaOCR for processing

            logging.info(f"Received image of size: {len(img_data)} bytes from {address}")

            writer.write(b'ACK')


            # Ensure the data is sent immediately
            # TODO: * Pode suspender indefinidamente se o peer ficar lento; use timeout (asyncio.wait_for) para evitar bloqueios.
                    # Pode levantar ConnectionResetError/BrokenPipeError/CancelledError se a conexão fechar.
                    # Não é necessário após cada write pequeno; prefira drain periódico ao streamar grandes payloads.
            await writer.drain()

    except asyncio.IncompleteReadError:
        logging.warning(f"Connection closed unexpectedly {address}")
    except Exception as e:
        logging.error(f"Error handling client {address}: {e}")
    finally:
        writer.close()
        logging.info(f"Connection closed {address}")

async def main():
    server = await asyncio.start_server(handle_client, HOST, PORT)
    address = server.sockets[0].getsockname()
    logging.info(f"Serving on {address}:{PORT}")

    async with server:
        await server.serve_forever()

if __name__ == '__main__':
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logging.info("Server shutting down...")