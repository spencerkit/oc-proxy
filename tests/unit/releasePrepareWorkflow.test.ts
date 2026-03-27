import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { mkdirSync, readFileSync } from "node:fs"
import path from "node:path"
import { test } from "node:test"

const repoRoot = process.cwd()
const workflowPath = path.join(repoRoot, ".github/workflows/release-prepare.yml")

function readWorkflow() {
  return readFileSync(workflowPath, "utf8")
}

function extractStepBlock(stepName: string) {
  const workflow = readWorkflow()
  const escapedStepName = stepName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
  const blockMatch = workflow.match(
    new RegExp(`- name: ${escapedStepName}[\\s\\S]*?run:\\s*\\|\\n((?: {10}.+(?:\\n|$))+)`)
  )

  if (blockMatch) {
    return blockMatch[1]
      .split("\n")
      .filter(Boolean)
      .map(line => line.slice(10))
      .join("\n")
  }

  const inlineMatch = workflow.match(new RegExp(`- name: ${escapedStepName}[\\s\\S]*?run:\\s*(.+)`))
  assert.ok(inlineMatch, `expected ${stepName} step in release-prepare workflow`)
  return inlineMatch[1].trim()
}

function extractReadVersionCommand() {
  return extractStepBlock("Read version")
}

test("release-prepare checks out the base branch via refs/heads to avoid tag ambiguity", () => {
  const workflow = readWorkflow()

  assert.match(
    workflow,
    /- name: Checkout[\s\S]*?ref:\s*\$\{\{\s*format\('refs\/heads\/\{0\}',\s*inputs\.base_branch\)\s*\}\}/
  )
})

test("release-prepare normalizes the checked out base branch name before create-pull-request", () => {
  const command = extractStepBlock("Normalize checked out base branch")

  assert.match(command, /git symbolic-ref HEAD --short/)
  assert.match(command, /\$\{CURRENT_BRANCH#heads\/\}/)
  assert.match(command, /git switch "\$NORMALIZED_BRANCH"/)
})

test("release-prepare removes a conflicting local tag before create-pull-request", () => {
  const command = extractStepBlock("Normalize checked out base branch")

  assert.match(command, /git rev-parse -q --verify "refs\/tags\/\$NORMALIZED_BRANCH"/)
  assert.match(command, /git tag -d "\$NORMALIZED_BRANCH"/)
})

test("release-prepare Read version command is shell-safe", () => {
  const outputDir = path.join(repoRoot, ".tmp")
  const outputPath = path.join(outputDir, "github-output-test.txt")
  const command = extractReadVersionCommand()

  mkdirSync(outputDir, { recursive: true })

  assert.doesNotThrow(() => {
    execFileSync("bash", ["-n", "-c", command], {
      cwd: repoRoot,
      env: { ...process.env, GITHUB_OUTPUT: outputPath },
      encoding: "utf8",
    })
  })
})
