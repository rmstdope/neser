---
name: skill-writer
description: Guide for creating and updating effective skills to be used for github copilot to extend its capabilities with specialized knowledge, workflows, or tool integrations.
---

# Skill Creator

This skill provides guidelines and best practices for creating and updating effective skills for github copilot.
It covers the entire skill creation process, from understanding the skill's purpose and planning its contents to writing the SKILL.md instructions and iterating based on real usage.

## About Skills

Skills are modular extensions that provide specialized knowledge, workflows, or tool integrations to github copilot.
They allow you to turn Copilot into an expert in a specific domain or task by providing it with targeted information and instructions.
Each skill consists of a SKILL.md file with metadata and instructions, along with optional bundled resources like scripts, references, and assets.
Skills are designed to be reusable and shareable, enabling you to build a library of capabilities that can be easily applied to relevant tasks.

### What Skills Provide

- **Specialized Knowledge**: Skills can contain domain-specific information, best practices, and heuristics that help Copilot make informed decisions in that area.
- **Workflows**: Skills can define step-by-step processes for handling complex tasks, guiding Copilot through the necessary steps to achieve the desired outcome.
- **Tool Integrations**: Skills can include scripts and assets that enable Copilot to interact with external tools, APIs, or services to perform specific functions.
- **Contextual Triggers**: The metadata in SKILL.md helps Copilot understand when to apply the skill, ensuring that it is used in the right situations.
- **Reusable Resources**: By bundling scripts, references, and assets, skills provide reusable components that can be leveraged across multiple tasks without needing to rewrite code or re-explain concepts.

## Core Principles

### Finite context window

Skills must be designed with the understanding that Copilot has a limited context window (currently around 8k tokens). This means:

- Only include information that is essential for the skill's functionality
- Use references files for detailed information that may not always be needed
- Avoid including large amounts of information in SKILL.md that may not be relevant to every use of the skill.

### Clear and concise instructions

- Use simple language and avoid jargon unless it is necessary for the skill's domain
- Break down complex processes into clear, step-by-step instructions
- Use examples to illustrate key points and provide clarity
- Avoid ambiguity and ensure that the instructions can be easily followed by Copilot

### Iterative improvement

- Monitor how the skill is being used and gather feedback on its effectiveness
- Regularly update the skill based on real usage to improve its performance and relevance
- Be open to making significant changes to the skill if it is not achieving the desired results, even if that means rewriting large portions of the SKILL.md or changing the included resources.
- Continuously refine the skill to ensure that it remains effective and useful over time.

### Repository structure

- Organize skills in a clear and consistent directory structure within the repository

Below is an example of how to structure a skill within the repository:

```
.github/
  skills/
    skill-name/
      SKILL.md
      references/
        reference1.txt
        reference2.txt
      scripts/
        script1.py
        script2.js
      assets/
        asset1.png
        asset2.json
```

#### SKILL.md Structure

The SKILL.md file should be structured with the following sections:

1. Metadata: Include the skill's name, description, and any relevant tags or categories.
   a. Example:

```
---
name: skill-name
description: A brief description of what the skill does and its intended use cases.
---
```

2. Introduction: Provide an overview of the skill, its purpose, and how it can be used.
3. Instructions: Detailed step-by-step instructions for how Copilot should apply the skill in relevant situations.
4. References: List any reference materials included with the skill and how they should be used.
5. Examples: Provide examples of how the skill can be applied in practice, including sample inputs
   and expected outputs.

#### Bundled Resources

- Include any scripts, references, or assets that are necessary for the skill's functionality in the appropriate subdirectories.
- Ensure that the SKILL.md instructions clearly explain how to use these resources and when they should be applied.
- Regularly review and update the bundled resources to ensure they remain relevant and effective for the skill's intended use cases.

#### Token Budget

Be mindful of the token budget when creating skills.

1. Metadata: Always in context so keep below 100 words.
2. Introduction: Keep it concise, ideally under 200 words, to provide a clear overview without overwhelming the context.
3. Instructions: Focus on essential steps and guidance, aiming for clarity and brevity. Use examples to illustrate complex points without needing lengthy explanations.
4. References: Use references to provide detailed information that may not be needed in every use of the skill, allowing you to keep the main instructions concise.
5. Examples: Provide a few well-chosen examples that demonstrate the skill's application without needing extensive explanations.

## Creating and Working with Skills

### Understanding the Skill's Purpose

Clearly define the problem or task that the skill is intended to address, perferably by identifying specific examples of when the skill would be useful. This will help guide the content and structure of the SKILL.md instructions.

### Planning the Skill's Contents

Outline the key information, instructions, and resources that the skill will include. Consider how to structure the SKILL.md file to effectively convey this information while keeping it concise and within the token budget.

### Writing the SKILL.md Instructions

Use clear and concise language to write the instructions for the skill. Break down complex processes into simple steps and use examples to illustrate key points. Ensure that the instructions are easy for Copilot to follow and apply in relevant situations.

### Creating and Bundling Resources

Include any necessary scripts, references, or assets that will support the skill's functionality. Ensure that these resources are well-organized and clearly referenced in the SKILL.md instructions.

### Iterating Based on Real Usage

Monitor how the skill is being used and gather feedback on its effectiveness. Regularly update the skill based on real usage to improve its performance and relevance. Be open to making significant changes to the skill if it is not achieving the desired results, even if that means rewriting large portions of the SKILL.md or changing the included resources. Continuously refine the skill to ensure that it remains effective and useful over time.
