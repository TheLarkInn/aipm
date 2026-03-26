---
description: >
  Research the codebase when the "research" label is applied to an issue,
  post findings back as the issue description, and relabel with "spec review".
on:
  label_command:
    name: research
    events: [issues]
permissions:
  contents: read
  issues: read
  pull-requests: read
tools:
  github:
    toolsets: [default]
safe-outputs:
  update-issue:
    target: "triggering"
    body: true
    max: 1
  add-labels:
    allowed: [spec review]
    max: 1
---
/aipm-atomic-plugin:research-codebase ${{ github.event.issue.title }}

