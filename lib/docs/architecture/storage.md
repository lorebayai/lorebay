# Storage

- **Status:** Proposed
- **Level:** System
- **Parent:** [Library architecture](library.md)
- **Related:** [Data model](data-model.md), [Tasks](../contracts/tasks.md),
  [Entry points](entry-points.md)

> Library code calls only its data layer and the file storage interface,
> never a database engine or a filesystem directly.
>
> Every rule about a file is the library's; a storage backend carries it out
> and decides none.

## 1. Purpose

Each product runs the library on the database and the file storage it
chooses. Two narrow abstractions, the data layer and the file storage
interface, keep the library the same for all of them.

## 2. Scope

This doc covers the file storage interface and the rules every file
follows, the data layer, transactions and search. The tables are in
[Data model](data-model.md); the interfaces' methods are in
[Library architecture](library.md#6-interfaces).

It does not cover:

- multipart upload of a file larger than one upload can carry;
- moving a product's data between backends.

## 3. File storage

- **The interface abstracts issuing URLs, not reading and writing bytes.**
  A backend issues short-lived URLs, and files travel between the caller and
  the backend without passing through the library.
- **Each artifact file is written once.** Each artifact has its own location
  in storage, which the library generates and never takes from the file
  name. Every backend answers 403 to an upload that lacks the headers its
  upload URL came with, and 412 to a second upload to a location, so a
  client that gets 412 knows the file is already stored. What a client sees
  is in [Tasks](../contracts/tasks.md#5-artifacts).
- **The client reports a finished upload.** A backend may not see when an
  upload finishes, so the client must call `complete_artifact` after the
  transfer. The library then asks storage for the file's size and records
  it.
- **The size limit is checked after the upload**, as
  [Library architecture](library.md#9-files) states.
- **The interface lists the stored files.** The cleanup round that deletes
  files no record points to finds them in that list, and the round that
  removes abandoned registrations deletes their files. A backend's own
  leftovers, such as partial uploads, are its own to remove.
- **Only the configured types open in the browser.** The library works out
  a download's content type from the file name's extension and serves only
  the types the product configures inline. Every other file, HTML and SVG
  included, is served as a download, never opened in the browser, so an
  uploaded file can never run inside a product's pages with a person's
  credential. The configured types are ones a browser runs no script in.

## 4. Database

- **The data layer reads and writes the library's tables** and nothing
  else. It runs on the two backends of
  [Library architecture](library.md#41-database-backends-and-dependencies),
  SQLite and Postgres, which share one schema built only from what both
  support; see [Data model](data-model.md#3-conventions).
- **Each write runs in one transaction the product opens.** An operation
  that spans several tables, such as approving a proposal, deleting a task,
  or taking the next key for a new record, writes inside it; see
  [Library architecture](library.md#7-writes-and-transactions).
- **Task and artifact deletion remove files after commit.** A file left
  behind is removed by the cleanup round for files without a record. The
  [oversize refusal](library.md#71-run-one-write) holds the artifact lock
  through the size recheck and deletion, leaving the registration incomplete.
- **Keyword search is substring matching.** It uses SQL `LIKE`, not
  full-text search. This is enough at team scale, and it matches Chinese
  text without splitting it into words. How it treats case on both backends
  is in [Library architecture](library.md#13-concurrent-writers).
