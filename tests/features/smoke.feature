@smoke
Feature: The app starts and its surfaces are reachable

  Breadth, not depth. This is the tier that runs on every release including
  patches, and it is the tier that would have caught most of what shipped
  broken: a canvas showing [object Object], a switch that silently refused, a
  tab that rendered nothing.

  A surface that cannot be reached is a failure. A surface that opens but is
  empty when it should not be is also a failure, and the screenshot is the
  evidence either way.

  Background:
    Given the latest release is installed
    And the app has been launched

  Scenario: The app launches and reports its version
    Then the window is visible
    And the version shown matches the release under test

  Scenario: Every top-level surface opens
    When I open each surface in the top bar in turn
    Then each one renders its own content rather than staying blank
    And no surface reports an error in the console
    And any surface I could not reach is reported as untested, not as passed

  Scenario: A project workspace opens and shows its tabs
    Given at least one project exists
    When I open that project
    Then the workspace shows its tabs
    And each tab I open renders content belonging to that tab

  Scenario: The Observatory lists what is actually running
    When I open the Observatory
    Then it lists the agents and sandboxes currently running on this machine
    And each row offers a way to attach to that session
    And the plan usage figures are present rather than blank

  Scenario: A terminal tab opens and runs a command
    When I open a new terminal tab
    Then a shell prompt appears
    When I run a command that prints a known string
    Then that string appears in the terminal output

  Scenario: Nothing in the interface shows a raw object or an error string
    When I visit every surface reachable without starting work
    Then no visible text contains "[object Object]"
    And no visible text contains "undefined" or "NaN" where a value belongs
    And no panel is showing a spinner that never resolves
