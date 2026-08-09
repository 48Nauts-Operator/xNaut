@multiagent @slow
Feature: Several agents work at once, and the app tells the truth about them

  The claim xNAUT makes is not that it runs one agent well; it is that it runs
  several at once, in real isolation, and reports their state honestly. Most of
  what has gone wrong here was reporting, not execution: healthy agents declared
  dead and killed, idle sessions animating as if mid-thought, a status poll that
  rebuilt every row three times a second.

  Background:
    Given the latest release is installed
    And a project that can host more than one agent

  Scenario: Several agents run at once without colliding
    When I start more than one agent on the same project
    Then each one runs in its own session
    And each one has its own working folder
    And none of them reports another's output

  Scenario: A running agent is never reported as dead
    Given agents that are running and doing work
    Then every one of them is shown as alive
    And none is killed or restarted while it is working
    And an agent whose state cannot be determined is treated as alive

  Scenario: An agent receives the instruction it was given
    When I start an agent with a specific goal
    Then the agent's first action reflects that goal
    And it does not begin by asking what it is supposed to do

  Scenario: State in the sidebar matches reality
    Given a mix of working, waiting and finished agents
    Then each one's state matches what its session is actually doing
    And exactly one state animates, so idle is visually distinct from busy
    And the list does not flash or rebuild itself while I watch it

  Scenario: A session outlives the app
    Given an agent running in a terminal session
    When I quit and reopen the app
    Then that session is still running
    And I can attach to it and see what it did while the app was closed

  Scenario: Killing a session actually kills it
    When I stop an agent from the project view
    Then its session ends
    And no process it started is left behind

  Scenario: Cost is visible while the work is happening
    Given agents that have been running long enough to consume budget
    Then the app shows what has been consumed
    And the figure changes as the work continues
