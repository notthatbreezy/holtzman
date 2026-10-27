"""Use SKIPPY's Ethernet route without changing its authorized Host header."""
from urllib.parse import urlparse

import httpx2


def ship_transport(url):
    options = {"trust_env": False}
    if urlparse(url).hostname in {"skippy", "skippy.local"}:
        # mDNS can also advertise an unusable link-local IPv6 route on this LAN.
        options["local_address"] = "0.0.0.0"
    return httpx2.AsyncHTTPTransport(**options)
