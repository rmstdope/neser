---
description: "Debug a NES game issue"
name: "debug-game"
argument-hint: "problemDescription (for example: sprite flickering on level 2 in rom x.nes)"
agent: "agent"
---

`${input:problemDescription}`
Find out if there are any discrepancies between our implementation of the mapper in question and the specification of that mapper on NesDev. Review the specification of the mapper and our implementation, and try to find any discrepancies between the two. If you find any discrepancies, fix them so that our implementation matches the specification.If no discrepancies are found, compare our implementation of the mapper with other known good implementations, such as those in popular emulators like Mesen, FCEUX or Nestopia. Look for any differences in behavior or edge cases that might be relevant to the issue at hand. If you find any differences, update our implementation to match the behavior of the known good implementations, and test if that resolves the issue. If no differences are found, analyze the game's code and behavior to identify any patterns or conditions that trigger the issue. Look for any specific instructions, memory accesses, or timing-related behaviors that could be contributing to the problem. Feel free to ask for help with tracing the game's execution or inspecting memory at specific points to gather more information about the issue.

When implementing, be sure to apply the `bug-hunter` skill.
