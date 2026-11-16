"""Minimal RESP2 client for authenticated infrastructure probes, without external packages."""
import socket


class RedisProbe:
    def __init__(self, host, port, password):
        self.host = host
        self.port = port
        self.password = password

    def command(self, *arguments):
        with socket.create_connection((self.host, self.port), timeout=10) as connection:
            with connection.makefile("rb") as stream:
                connection.sendall(self._encode(("AUTH", self.password)))
                if self._reply(stream) != "OK":
                    raise RuntimeError("Dragonfly authentication failed")
                connection.sendall(self._encode(arguments))
                return self._reply(stream)

    @staticmethod
    def _encode(arguments):
        parts = [f"*{len(arguments)}\r\n".encode()]
        for argument in arguments:
            value = str(argument).encode()
            parts.extend([f"${len(value)}\r\n".encode(), value, b"\r\n"])
        return b"".join(parts)

    @staticmethod
    def _reply(stream):
        line = stream.readline(4097)
        if len(line) > 4096 or not line.endswith(b"\r\n"):
            raise RuntimeError("Invalid Dragonfly response")
        marker, value = line[:1], line[1:-2]
        if marker == b"-":
            raise RuntimeError("Dragonfly rejected the probe command")
        if marker == b"+":
            return value.decode()
        if marker == b":":
            return str(int(value))
        if marker == b"$":
            size = int(value)
            if size == -1:
                return ""
            if not 0 <= size <= 1_048_576:
                raise RuntimeError("Dragonfly response exceeds probe limit")
            data = stream.read(size + 2)
            if len(data) != size + 2 or not data.endswith(b"\r\n"):
                raise RuntimeError("Incomplete Dragonfly response")
            return data[:-2].decode()
        raise RuntimeError("Unsupported Dragonfly response")
