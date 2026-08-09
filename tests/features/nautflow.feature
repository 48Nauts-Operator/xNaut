@nautflow @slow
Feature: A request becomes a merged change, and the build can prove what it did

  From the v1.13.0 notes: the build stage exists so that a build can tell you
  what it did. The gate reports a score rather than a verdict, slices declare
  dependencies, and the log outlives the run.

  This tier costs real agent tokens and tens of minutes. It runs on feature
  releases, not patches.

  Background:
    Given the latest release is installed
    And a project with a documentation chain that the Validator accepts

  Scenario: The Validator blocks a build on a broken chain
    Given a project whose documentation chain is incomplete
    When I start a build
    Then the build does not start
    And the report names which stage is incomplete
    And I can read that report in the app

  Scenario: A build runs several slices in parallel, each isolated
    When I start a build with more than one independent slice
    Then each slice gets its own worktree on its own branch
    And each slice has its own terminal session I can watch
    And two slices never write to the same working folder

  Scenario: A slice that depends on another waits rather than failing
    Given a build plan where one slice depends on another
    Then the dependent slice does not start until its dependency completes
    And while waiting it holds no worktree
    And if the dependency fails, the dependent slice is reported unreachable rather than failed

  Scenario: The gate reports a score, not a verdict
    When a build reaches its acceptance gate
    Then the result is passed-of-total rather than yes or no
    And a gate that crashed reports no score at all rather than zero

  Scenario: The build log survives the run
    Given a build that has finished
    When I reopen its log afterwards
    Then every event is still there with its level, source and timestamp
    And I can filter by level and by slice
    And the manager's decisions are readable, not just its last message

  Scenario: The Files view measures against the merge base
    Given a slice whose agent has committed its work
    When I open the Files view for that slice
    Then it shows the files that slice changed
    And the count is measured from where the slice forked, not from the working folder
    And an agent that has committed is not reported as idle

  Scenario: A dead slice stops the build honestly
    Given a build where one slice cannot be completed
    Then the build reports which slice died and why
    And consolidation refuses to merge rather than shipping the survivors

  @destructive
  Scenario: A green build merges and opens a pull request
    Given a build where every slice has passed its gate
    When consolidation runs
    Then the slices are merged
    And a pull request is opened describing what was built
