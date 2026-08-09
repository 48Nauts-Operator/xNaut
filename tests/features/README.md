# Release test scenarios

Gherkin for an **agent**, not for a step-definition runner. There is no glue
code and there is not meant to be. A resident agent on the test machine reads
these, works out how to satisfy each step with whatever harness it has, and
reports a verdict per scenario.

## Why Gherkin, given there is no runner

Because it is intent without mechanics. `Then the canvas shows the running site`
is unambiguous as a pass condition and says nothing about how to click. That
makes the same file valid whether the agent drives Chrome through Browser
Harness, the native app through OS-level control, or something later.

It also gives, for free, the three things an unattended test needs: coverage is
the scenario list, the verdict is per `Then`, and the same files across releases
make two runs comparable.

## These are written from the guide, never from the code

Scenarios derived from the implementation assert what the code *does*, so they
pass on a broken build. Every one of these comes from the changelog, the release
notes and the docs: what the thing is *supposed* to do.

That means the code and these files can disagree, and **the disagreement is the
finding**, not something to reconcile away:

- a surface in the app with no scenario is untested, not passing
- a scenario with no implementation is a regression, or the docs are lying

## Rules the agent follows

1. **A surface that was never reached is `untested`, not `passed`.** The report
   must distinguish them. Not doing so is exactly how the Designer stayed broken
   through three releases that were reported as fixed.
2. **Every verdict carries evidence.** A screenshot for anything visual, the log
   path for anything asynchronous. A claim with no artifact is not a result.
3. **Report what actually happened, including your own failures.** "I could not
   reach this screen" is a useful result. A pass you did not observe is not.

## Tags

| Tag | When it runs | Cost |
|---|---|---|
| `@smoke` | every release, including patches | a few minutes |
| `@designer` | any release touching the Designer | minutes, needs a sandbox or local runtime |
| `@nautflow` | feature releases | tens of minutes, real agent tokens |
| `@multiagent` | feature releases | tens of minutes, real agent tokens |
| `@slow` | on request | anything above ten minutes |
| `@destructive` | never unattended | deletes or pushes something |

A patch release runs `@smoke`. A feature release runs everything except
`@destructive`.
