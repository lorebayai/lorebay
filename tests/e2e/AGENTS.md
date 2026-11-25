Follow these playbook fragments:

- @../../playbook/topics/testing.md
- @README.md

## E2E conventions

- A journey creates its own space and data and runs independently of other journeys.
- People act through the compiled CLI or Chromium pages; agents act through MCP.
- Share CLI, browser authorization and file transfer mechanics with deployment tests in `../common/`.
