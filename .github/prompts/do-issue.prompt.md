---
description: "Start implementation planning for a GitHub issue number with mandatory clarification questions when needed"
name: "do-issue"
argument-hint: "issueNumber (for example: 1585)"
agent: "plan"
---

Lets start working on #${input:issueNumber}.
Start by making a plan for what we should do and how it should be implemented. Be sure to ask me questions using the question UI/tool if there are any uncertainties on how to implement or there are design decisions that need to be made.
If need be, research NES specifications with NesDev docs as the primary authoritative source. If that is not reachable, use its archive mirror, https://nesdev-wiki.nes.science/wikipages/Special_AllPages.xhtml
When implementing, be sure to follow TDD according to instructions in the `test-driven-development` skill, and to apply the `bug-hunter` skill if you encounter any bugs during implementation.
