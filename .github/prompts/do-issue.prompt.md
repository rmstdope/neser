---
description: "Start implementation planning for a GitHub issue number with mandatory clarification questions when needed"
name: "do-issue"
argument-hint: "issueNumber (for example: 1585)"
agent: "agent"
---

lets start working on #${input:issueNumber}. start by making a plan for what we should do and how it should be implemented. be sure to ask me questions using the question ui/tool if there are any uncertainties on how to implement or there are design decisions that need to be done. to start with:
A new config option --enable-4-score should be added. If that is set, the player will _have_ to have at least two controllers connected to the computer so that emulated controller 1-2 can use the physical controllers and emulated controller 3-4 can use the keyboard. Should one controller by unplugged during emulation, only the first three emulated controllers will work. should both be unplugged, only the keyboard will emulate controller 1-2.
