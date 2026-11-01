"""Use SKIPPY's Ethernet route without changing its authorized Host header."""
from urllib.parse import urlparse

import httpx2


def ship_transport(url, *, connect_retries=0):
    options = {"trust_env": False}
    if connect_retries:
        # Retry connection establishment only, never replay a dispatched request.
        options["retries"] = connect_retries
    if urlparse(url).hostname in {"skippy", "skippy.local"}:
        # mDNS can also advertise an unusable link-local IPv6 route on this LAN.
        options["local_address"] = "0.0.0.0"
    return httpx2.AsyncHTTPTransport(**options)
