# Storage

- **Status:** Proposed
- **Level:** System
- **Parent:** [System architecture](system.md)
- **Related:** [Data model](data-model.md),
  [Library storage](../../lib/docs/architecture/storage.md)

> A default deployment runs with no external services: SQLite and a local
> directory, both in one data directory.
>
> Storage backends are chosen at deployment and never change afterward.

## 1. Purpose

A default deployment must run with no external services, while a team that
wants its data on external services can use S3 for files and Postgres for
the database. The library's data layer and file storage interface, in
[Library storage](../../lib/docs/architecture/storage.md), keep the rest of
`lorebayd` the same for all of them.

## 2. Scope

This doc covers the backends Lorebay runs, its data directory and its
backups. The rules every file and every transaction follow are in
[Library storage](../../lib/docs/architecture/storage.md); Lorebay's tables
are in [Data model](data-model.md).

It does not cover:

- moving an existing deployment between backends, such as from a local
  directory to S3, or from SQLite to Postgres;
- storage services other than Amazon S3 that offer the S3 API, such as
  MinIO: the S3 backend relies on Amazon S3's conditional writes.

## 3. Layers

| Layer | Default backend | Other backend |
| --- | --- | --- |
| Database | SQLite | Postgres |
| File storage | Local directory | S3 |

- **No external dependencies by default.** `lorebayd` starts with a data
  directory that holds the SQLite file and the artifact files.
- **An earlier data directory must be recreated when startup refuses it.**
  Move the old directory aside and start with an empty one. Keep the users
  file outside the old directory so `lorebayd` can recreate the accounts.
  Stored work and authorizations stay in the old directory.
- **One database holds the library's tables and Lorebay's.** `lorebayd`
  turns on the library's backend that matches its database, and hands the
  library the transactions it opens.
- **A deployment runs on one host.** Every backend assumes a single
  `lorebayd` process.
- **Backups of the data directory need the service stopped.** Copy it only
  while `lorebayd` is stopped; a copy taken while it runs may hold a corrupt
  SQLite file.

## 4. File storage

- **With a local directory**, `lorebayd` issues short-lived URLs and serves
  the files itself. It receives an upload into a temporary file in the data
  directory and flushes it to disk. It then hard-links the file into place,
  which fails if a file is already there, and removes the temporary file. It
  rejects an upload without the header the upload URL came with, and an
  upload over the size limit. An interrupted upload leaves only the
  temporary file, which a background job removes; see
  [System architecture](system.md#6-background-jobs).
- **Every download is checked against its record.** The file URLs
  `lorebayd` serves ask the library whether the artifact is still recorded,
  so a URL stops working once its artifact is deleted.
- **With S3**, `lorebayd` issues presigned URLs, and files never pass
  through `lorebayd`. It signs `If-None-Match: *` into each upload URL. S3
  rejects an upload that leaves the header out, and refuses to write over an
  existing object.
- **An artifact must not exceed 5 GB**, the limit of a single S3 upload, and
  the most the size limit can be set to.
- **PNG, JPEG, GIF and WebP images and PDFs open in the browser**: they are
  the types Lorebay configures by default, under the library's
  [inline rule](../../lib/docs/architecture/storage.md#3-file-storage).

## 5. References

- [Amazon S3: preventing object overwrites with conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html):
  an upload with `If-None-Match: *` fails with 412 when the key already
  exists, and must be signed with Signature Version 4.
