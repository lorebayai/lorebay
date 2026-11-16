# Storage

- **Status:** Proposed
- **Level:** System
- **Parent:** [System architecture](system.md)
- **Related:** [Data model](data-model.md), [Tasks](../contracts/tasks.md)

> Core code calls only the database layer and the file storage layer, never
> SQLite or the filesystem directly.
>
> Storage backends are chosen at deployment and never change afterward.

## 1. Purpose

A default deployment must run with no external services, while a team that
wants files on an external service can use S3. Two narrow abstractions let
the core stay the same for both.

## 2. Scope

This doc covers the database layer, the file storage layer, their backends,
transactions and search. The tables are in [Data model](data-model.md).

It does not cover:

- Postgres: the database layer has only SQLite;
- moving an existing deployment between backends, such as from a local
  directory to S3, or from SQLite to Postgres;
- storage services other than Amazon S3 that offer the S3 API, such as
  MinIO: the S3 backend relies on Amazon S3's conditional writes;
- multipart upload of files over 5 GB.

## 3. Layers

| Layer | Abstracts | Default backend | Other backend |
| --- | --- | --- | --- |
| Database layer | Reads and writes per table, transactions, keyword search | SQLite | None |
| File storage layer | Issue an upload URL and the headers it requires, issue a download URL, check a file's existence and size, delete a file | Local directory | S3 |

- **No external dependencies by default.** `lorebayd` starts with a data
  directory that holds the SQLite file and the artifact files.
- **A deployment runs on one host.** Both backends assume a single
  `lorebayd` process.
- **Backups need the service stopped.** Copy the data directory only while
  `lorebayd` is stopped; a copy taken while it runs may hold a corrupt
  database file.

## 4. File storage

- **The layer abstracts issuing URLs, not reading and writing bytes.** With a
  local directory, `lorebayd` issues short-lived URLs and serves the files
  itself. With S3, it issues presigned URLs, and files never pass through
  `lorebayd`.
- **Each artifact file is written once.** Each artifact has its own location
  in storage, which Lorebay generates and never takes from the file name.
  What a client sees is in [Tasks](../contracts/tasks.md#5-artifacts).
- **With S3**, `lorebayd` signs `If-None-Match: *` into the presigned URL.
  S3 rejects an upload that leaves the header out, and refuses to write over
  an existing object.
- **With a local directory**, `lorebayd` receives the upload into a
  temporary file in the data directory and flushes it to disk. It then
  hard-links the file into place, which fails if a file is already there,
  and removes the temporary file. Like S3, it rejects an upload without the
  header, and it rejects uploads over 5 GB. An interrupted upload leaves only
  the temporary file, which a background job removes; see
  [System architecture](system.md#6-background-jobs).
- **The client reports a finished upload.** With S3, `lorebayd` cannot see
  when an upload finishes. With either backend, the client must call
  `complete_artifact` after the transfer. `lorebayd` then checks that the
  file exists in storage and records its size.
- **An artifact must not exceed 5 GB**, the limit of a single S3 upload.
  Larger files would need multipart upload.
- **Files that can run scripts are served as downloads.** Images other than
  SVG, and PDFs, open in the browser. HTML, SVG and other files that can run
  scripts are always served as downloads, never opened in the browser. An
  uploaded HTML file therefore can never run inside Lorebay's pages with the
  login cookie. Both backends follow this rule.

## 5. Database

- **The database layer provides transactions.** An operation that spans
  several tables runs in one transaction, such as approving a proposal,
  deleting a task, or taking the next key for a new record.
- **Files are outside transactions.** Deleting a task or an artifact commits
  the database first, then deletes the files. A background job removes files
  left behind; see [System architecture](system.md#6-background-jobs).
- **Keyword search is substring matching.** It uses SQL `LIKE`, not
  full-text search. This is enough at team scale, and it matches Chinese
  text without a tokenizer.

## 6. References

- [Amazon S3: preventing object overwrites with conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html):
  an upload with `If-None-Match: *` fails with 412 when the key already
  exists, and must be signed with Signature Version 4.
