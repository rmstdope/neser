---
description: "Start implementation planning for a GitHub issue number with mandatory clarification questions when needed"
name: "plan-issue"
argument-hint: "issueNumber (for example: 1585)"
agent: "Plan"
---

I want to make a plan for implementing GitHub issue #${input:issueNumber}.

Interview me relentlessly about every aspect of this plan until we reach a shared understanding. Walk down each branch of the design tree, resolving dependencies between decisions one-by-one.

Ask the questions using the question UI/tool and for each question, provide your recommended answer. Ask the questions one at a time.

If a question can be answered by exploring the codebase, explore the codebase instead.

Once you have a clear and agreed plan, summarize it and ask me to confirm before proceeding with implementation.
