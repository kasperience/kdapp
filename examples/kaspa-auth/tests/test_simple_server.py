import threading
from http.server import HTTPServer, SimpleHTTPRequestHandler
import http.client
import time


def run_server(server):
    try:
        server.serve_forever()
    except Exception:
        pass


def test_simple_python_http_server_lifecycle():
    # Bind to an ephemeral port on localhost
    server = HTTPServer(("127.0.0.1", 0), SimpleHTTPRequestHandler)
    host, port = server.server_address

    t = threading.Thread(target=run_server, args=(server,), daemon=True)
    t.start()

    # Give the server a moment to start
    time.sleep(0.1)

    # Make a simple GET request to the root
    conn = http.client.HTTPConnection(host, port, timeout=2)
    conn.request("GET", "/")
    resp = conn.getresponse()
    assert resp.status == 200
    conn.close()

    # Shutdown and ensure the thread terminates
    server.shutdown()
    server.server_close()
    t.join(timeout=2)
    assert not t.is_alive()

