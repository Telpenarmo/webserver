# Webserver

Almost-minimal implementation of HTTP server in Rust.

Started as a port-to-Rust from project for Computer Networks course.
I develop it further just for the fun of it, so making it production-ready is an explicit non-goal.
However, *production-ready* is often close to *better*, and making things *better* is often *fun*, so I expect this project to slowly mature.

## Usage

The usage could be summarized in following line:

```sh
cargo run $content-directory -p $port
```

However, there are a few points definitely worth mentioning.

On startup, *Webserver* looks for **subdirectories** of `content-directory` and tries to treat each of them as separate host, binding its name and selected port for listening.
This means that if you want to be able to access your page by `http://localhost:{port}/index.html` you need to have at least following structure:

```text
$content-directory
└── localhost
    └── index.html
```

More real example would be

```text
$content-directory
└── localhost
    ├── index.html
    ├── css
    │   └── styles.css
    └── other-page.html
```

where styles file can be accessed with `http://localhost:{port}/css/styles.css`.

Having something like

```text
$content-directory
└── your-domain
    └── index.html
```

allows *webserver* to use `http://your-domain:{port}/index.html`.
Under condition that your-domain points to something on `127.*.*.*`, for example under `/etc/hosts`, of course.

It is impossible to host files under plain IP address, with no domain.
You can access your files by IP, however.

This is, as You surely noticed, quite strange and not very useful.
*Webserver* inherits that from his uni-project ancestor.
This doesn't hurt me in any way, so I am not planning to change it.

## Features

- currently only GET and HEAD methods are supported
- many hosts, each using its own thread
- keeping connection alive for some time
- separate thread pool for each host
- graceful shutdown
- per-host and global error pages ({status_code}.html)
- some other, I'll update that list someday

## To Do

- [ ] tests
- [x] use flags for optional configuration
- [ ] support more HTTP methods:
  - [x] HEAD
  - [ ] PUT
  - [ ] POST
  - [ ] DELETE
- [ ] support dynamic hosts
- [x] setup proper logging
