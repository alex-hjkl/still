#!/usr/bin/env python3
"""Inspect the Subsonic paths returned for Still; uses only Python's standard library."""

import getpass
import hashlib
import json
import os
import secrets
import sys
import urllib.error
import urllib.parse
import urllib.request


class Navidrome:
    def __init__(self, server, username, password):
        parsed = urllib.parse.urlsplit(server.strip())
        if parsed.scheme not in ("http", "https") or not parsed.hostname:
            raise ValueError("Enter the full URL: http://… or https://…")
        if parsed.username is not None or parsed.password is not None:
            raise ValueError("Enter username and password separately from the URL")
        if not username.strip() or not password:
            raise ValueError("Username and password must not be empty")
        self.base = urllib.parse.urlunsplit(
            (parsed.scheme, parsed.netloc, parsed.path.rstrip("/") + "/", "", "")
        )
        salt = secrets.token_hex(12)
        self.auth = {
            "u": username.strip(),
            "t": hashlib.md5((password + salt).encode("utf-8")).hexdigest(),
            "s": salt,
            "v": "1.16.1",
            "c": "Still",
            "f": "json",
        }

    def request(self, method, **params):
        endpoint = urllib.parse.urljoin(self.base, f"rest/{method}.view")
        url = endpoint + "?" + urllib.parse.urlencode(self.auth | params)
        # Still's reqwest client sends no User-Agent. Navidrome matches players by
        # user, client name AND User-Agent; suppress urllib's default to match it.
        opener = urllib.request.build_opener()
        opener.addheaders = []
        request = urllib.request.Request(url, headers={"User-Agent": ""})
        with opener.open(request, timeout=30) as response:
            body = json.load(response)
        result = body.get("subsonic-response")
        if not isinstance(result, dict):
            raise ValueError("Unexpected Subsonic API response. Check the Navidrome URL")
        if result.get("status") != "ok":
            code = result.get("error", {}).get("code", "?")
            message = "Invalid username or password" if code in (40, 41) else "Navidrome error"
            raise ValueError(f"{message} (code {code})")
        return result


def show_paths(client, query):
    body = client.request(
        "search3", query=query, artistCount=0, albumCount=0, songCount=20, songOffset=0
    )
    songs = body.get("searchResult3", {}).get("song", [])
    print(f"\nNavidrome {body.get('serverVersion', body.get('version', '?'))} · client Still")
    if not songs:
        print("No songs found. Try part of a song title or artist name.")
    for song in songs:
        print(f"\n{song.get('title', '?')} — {song.get('artist', '?')}")
        print(f"  Album: {song.get('album', '—')}")
        print(f"  path:   {song.get('path') or '[server did not return a path]'}")
    if len(songs) == 20:
        print("\nShowing the first 20 results. Narrow the query for an exact match.")
    print("\nCompare path with the actual location of this file on the server.")
    print("In Docker, paths refer to the container: for example, /music/Collections/…")
    print("A path-shaped value does not prove it is real: check the directory and filename.")


def main():
    server = os.environ.get("NAVIDROME_URL") or input("Navidrome URL: ")
    username = os.environ.get("NAVIDROME_USERNAME") or input("Username: ")
    password = os.environ.get("NAVIDROME_PASSWORD") or getpass.getpass("Password: ")
    client = Navidrome(server, username, password)
    del password
    client.request("ping")
    print("\nConnected. Still should now appear in the Navidrome Players settings.")
    print("Enable Report Real Path for Still and repeat the query here to compare responses.")
    while True:
        query = input("\nPart of a song title / artist (Enter for first 20, :q to quit): ").strip()
        if query == ":q":
            return
        show_paths(client, query)


if __name__ == "__main__":
    try:
        main()
    except (KeyboardInterrupt, EOFError):
        print("\nCheck finished.")
    except urllib.error.HTTPError as error:
        print(f"HTTP error {error.code}. Check the URL and API availability.", file=sys.stderr)
        sys.exit(1)
    except (ValueError, OSError, urllib.error.URLError):
        # Exceptions from HTTP libraries may contain the authenticated URL.
        print("Request failed. Check the URL, credentials and API response.", file=sys.stderr)
        sys.exit(1)
