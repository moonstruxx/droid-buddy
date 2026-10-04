### Requirement: Arrangement survives section reorder (MODIFIED)

Pin anchors SHALL be carried across graph rebuilds by circuit identity, so reordering sections (optimizer preview) never maps a pinned node to another node's previous position.

#### Scenario: Preview keeps pinned nodes on their own spots

- WHEN a preview reorders sections and rebuilds the graph
- THEN every still-present pinned circuit keeps its own previous position as its anchor, and unpinned nodes arrange around those anchors.
