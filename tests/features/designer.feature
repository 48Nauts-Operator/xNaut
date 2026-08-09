@designer
Feature: A design becomes a running site

  From the v1.11.0 and v1.13.4 release notes: a design is never a mock. The
  canvas shows a real running project, and in sandbox mode the URL is shareable
  as it stands.

  Every scenario here maps to something that actually shipped broken, so none of
  it is hypothetical. The Designer was reported fixed three times in one day
  while the canvas was showing a holding page over a finished site.

  Background:
    Given the latest release is installed
    And a project exists to hold designs

  @smoke
  Scenario: A new design defaults to local and says where it runs
    When I create a new design
    Then the header shows Local selected rather than Sandbox
    And the runtime shown is the one recorded for that design

  @smoke
  Scenario: The runtime switch either switches or explains why not
    Given a design that is not currently running
    When I switch it between Local and Sandbox
    Then the selection changes and is still correct after reopening the design
    Given a design that is currently running
    When I try to switch its runtime
    Then it is refused with a reason I can read, rather than silently ignored

  Scenario: A local design serves something within seconds
    Given a design set to run locally
    When I ask it to build anything
    Then within a minute the canvas shows a served page rather than a spinner
    And the address shown is on this machine
    And the page is reachable

  Scenario: The canvas shows the finished site, not the placeholder
    Given a local design whose agent has finished scaffolding a project
    Then the canvas shows that project, not the "Preparing this design" page
    And the address shown is the one actually serving the project

  Scenario: A status message never replaces the canvas
    Given a design that is serving a page
    When the app reports progress
    Then the served page is still visible
    And the progress text is readable somewhere on screen
    And no status text is rendered as a raw object

  Scenario: Stopping a design frees what it was using
    Given a design that is running locally
    When I stop it
    Then the address it was serving stops answering
    And no server it started is left running

  @slow
  Scenario: A sandbox design produces a shareable URL
    Given a design set to run in a sandbox
    When I ask it to build anything
    Then the canvas eventually shows the site on a public address
    And that address answers from outside this machine
    And if spin-up fails, the reason names the step that failed

  Scenario: Failures name the step and the duration
    Given a design whose spin-up does not succeed
    Then the message says which step failed and how long it took
    And the durable log for that design contains the same detail
