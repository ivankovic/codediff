# CodeDiff Design

This document describes the design details. For general project principles and guidelines, please
read the README.md in the root of the project.

# TUI

The UI displays the before and after code pairs. You can move around the code by using the arrow
keys or the vim hjkl navigation.

As you move around, the code under the cursor will be colour coded as follows:

If the code is red, you are in the before code and the code under the cursor is mapped as deleted.

If the code is green, you are in the after code and the code under the cursor is mapped as added.

If the code is yellow, the code under the cursor is mapped, but the mapping operation is not
"Identical", i.e. the code was moved or updated. Hit the space bar to move the other side of the
diff to show you the mapped code. It should be highlighted on both sides.

If the code is using the normal colour for the theme, the code is mapped as "Identical", i.e. it was
not changed. You can still hit the space bar to allign both sides of the diff.

If you would like to see the AST branch going from the root to the node currently under the cursor,
hit 't'. It will pop-up a floating display showing you the AST fragment starting from the root node
to the current node. Hitting the escape key or t again will close the pop-up.

## Framework

The Terminal User Interface is written using the ratatui library using crossterm as the terminal
control provider.

The automated tests for the TUI use the TestBackend from ratatui and insta for snapshot testing and
crossterms mock backend to simulate terminal input/output.

The tests must cover at least all user flows described in this document.

## Adaptive

The interface automatically adapts to the size of the terminal.

If the terminal is fewer than 220 characters wide, the interface is in "narrow mode".
In narrow mode, only a single code is displayed, either the before or the after code.
The tab key is used to switched between before and after.

If the terminal is 220 characters or wider, both before and after is displayed side by side. Tab key
is still used to switch the cursor between left and right hand side.


