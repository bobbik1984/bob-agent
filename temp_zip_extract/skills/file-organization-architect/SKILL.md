---
name: file-organization-architect
description: Standardized directory structure and file naming conventions. Use this skill WHENEVER generating code projects, documentation, export files, or handling assets to ensure a clean, universal workspace architecture.
version: 1.0.0
tags: [Development, Data, Design, Product]
related_skills: []
---

# File Organization Architect

A mandatory standard operating procedure (SOP) for file and project management within the workspace. 

When generating artifacts (e.g., Python projects, Javascript/Vue apps, Markdown docs, PPTX, or downloaded assets), you must place them in designated directories instead of scattering them in the root workspace or arbitrary subfolders.

## Architecture Guidelines

Always adhere to the following directory structure inside the `workspace` root:

### 1. Source Code & Projects => `/workspace/projects/{project_name}/`
When scaffolding a new coding project or script:
- Create a dedicated folder for it: e.g., `/workspace/projects/flight_tracker/`.
- Ensure all source code (`.py`, `.js`, `.json`), `Dockerfile`, and `requirements.txt` / `package.json` are encapsulated within that folder.
- **NEVER** write source code directly to the `/workspace/` root.

### 2. Documentation & Text => `/workspace/docs/{category}/`
When generating markdown logs, reports, test results, or technical plans:
- Store them under `/workspace/docs/`.
- Try to categorize them: e.g., `/workspace/docs/planning/`, `/workspace/docs/reports/`, etc.

### 3. Binary & Deliverables => `/workspace/exports/`
When generating binary files intended for the user to download or distribute:
- Word documents (`.docx`), Presentations (`.pptx`), Excel (`.xlsx`), or compiled PDFs should go to `/workspace/exports/`.

### 4. Assets & Media => `/workspace/assets/`
When downloading images, generating temporary `.webp` files, or handling raw media:
- Store them in `/workspace/assets/`. If they belong specifically to a coding project, they can go inside `/workspace/projects/{project_name}/assets/` instead.

## Execution Rules
- Explicitly create the directories if they do not exist before writing files to them.
- If the user explicitly specifies an absolute path, you may obey the user's path. Otherwise, fall back strictly to this architecture.
- Keep filenames lowercased, using hyphens or underscores instead of spaces (e.g., `market-research-2026.pptx` rather than `Market Research 2026.pptx`).

## Examples

**Example 1:**
*Input:* "Write a python script that tracks my budget."
*Action:* You should create `/workspace/projects/budget_tracker/main.py`.

**Example 2:**
*Input:* "Generate a PPTX slide deck about Space Exploration."
*Action:* You should create the file at `/workspace/exports/space_exploration.pptx`.
