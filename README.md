# Faasten CLI Client

This is a simple remote command-line client for the Faasten datastore.

The client supports five basic sub-commands commands:

```sh
    login    Login to Faasten
    get      Get the value of a database key
    put      Put a "blob" from a local file
    fetch    Download a "blob" to a local file
    set      Set the value of a database key from the provided value or standard in
```

Login credentials are stored in `$XDG_CONFIG`/fstn/credentials as a TOML-formatted file. Once logged in, you can perform the other operations.

## Values vs. Blobs

Faasten distinguishes between _values_ and _blobs_, with the former intended for
smaller, typically JSON, data that might change frequently and the latter
intended for larger binary data, such as tarballs. Both are stored at database
keys, but values are stored directly, while blobs are stored separately and
referred by the SHA256 hash of their content in the database.

As a result, you can `get` a blob but instead of the actual content you'll see a
SHA256 hash. `fetch`ing is typically more useful.

## Getting and Setting Values

Getting a key will print its value to standard out.

```sh
$ fstn get myvalue
{ "hello": "world" }
```

You can set values by either passing the value on the command line or via standard in.

```sh
$ fstn set myvalue '{"hello": "world"}'
```

```sh
$ echo '{"hello": "world"}' | fstn set myvalue
```

## Fetching and putting Blobs

Fetch blobs using the database key that refers to them and a file to store the output.

```sh
$ fstn fetch key/for/tarball output.tgz
```

Similarly, put a blob by providing a key to reference it and a file to put

``` sh
$ fstn put key/for/tarball local_tarball.tgz
```
