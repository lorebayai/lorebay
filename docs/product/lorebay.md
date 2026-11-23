# Lorebay

- **Status:** Proposed
- **Level:** System
- **Related:** [System architecture](../architecture/system.md),
  [Access control](../contracts/access-control.md),
  [Library](../../lib/docs/product/library.md)

> Lorebay is the first product built on the library: one deployment gives a
> team the library's spaces, with its own sign-in, web pages and skill.
>
> Anyone who can sign in may create a space.

## 1. Purpose

Lorebay brings the library to a team that wants no outside services: one
process holds the people who sign in, their spaces, the web pages they read
and the endpoints their agents call. The problem the library solves, spaces
and the data they hold are in [Library](../../lib/docs/product/library.md).

## 2. Scope

This doc covers what one Lorebay deployment carries and who may create a
space. Lorebay does only what its design docs and the library's describe;
anything they don't describe is out of scope.

It does not cover:

- the layout of the web pages;
- a Lorebay-wide administrator who can list spaces or reassign their
  owners;
- non-functional requirements, such as supported scale or data retention.

## 3. What a deployment carries

- **Many spaces.** A Lorebay holds many spaces, and every person who can
  sign in may belong to several.
- **Web pages.** Deploying Lorebay gives the data, the APIs and the pages
  people read, from one process. See
  [System architecture](../architecture/system.md).
- **`v0.1.0` serves only the authorization and device sign-in pages.** The
  [`lorebay` CLI](../contracts/cli.md) and the skill's
  [README](../../skill/README.md) cover what the other pages will, and those
  pages come in a later release. The CLI prints as plain text two things
  the library leaves to a product to show: a request marked "changed since
  requested", without its difference from the current content
  ([Knowledge](../../lib/docs/contracts/knowledge.md#4-requests)), and
  references, without links
  ([Keys and references](../../lib/docs/contracts/keys-and-references.md#5-references)).
- **Skill.** Lorebay writes and ships its own skill, which states the
  library's [agent rules](../../lib/docs/contracts/agent-rules.md) and
  Lorebay's additions in [Agent skill](../contracts/agent-skill.md). The
  skill's [README](../../skill/README.md) explains how to install the skill
  and connect an agent's MCP client.

## 4. Creating a space

- **Anyone who can sign in may create a space**, and becomes its owner.
- **Space members.** Who is in a space with which role, and how
  that changes, is in [Access control](../contracts/access-control.md).
