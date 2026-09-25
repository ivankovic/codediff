# Overview

As of commit 4c3d31df on main, 2026-09-25 11:01.

## Facts
- 9068 files, 5575 tests in 1406 files, 19 marked to skip

## Start here
- src/main.rs — the binary starts here
- src/lib.rs — the library's root, and its module map
- Cargo.toml — what it is called, and what it depends on
- Makefile — the tasks: build, test, install
- src/bin/analyze_human_mappings.rs — another binary
- src/bin/apted_only_benchmark.rs — another binary
- src/bin/apted_only_worker.rs — another binary
- src/bin/ascii_visualizer.rs — another binary

## What the rest of it leans on
- src/test/helper/human_mapping.rs — assert_matches_human_mapping, assert_matches_human_painting_within_limit, assert_matches_human_mapping_within_limit, Caches
- src/test/data/diffs/defects4j/java-defects4j-closure-110-node/after.java.test — getFirstChild, getNext, getLastChild, hasChildren
- src/test/data/diffs/defects4j/java-defects4j-closure-110-node/before.java.test — getFirstChild, getNext, getLastChild, hasChildren
- src/test/data/diffs/stratified/typescript-microsoft-typescript-add-one-line/after.ts.test — System, EnumType, Modifier, Block
- src/test/data/diffs/stratified/typescript-microsoft-typescript-add-one-line/before.ts.test — System, EnumType, Modifier, Block
- src/review.rs — Commit, Review, ChangeSet, ReviewTarget
- src/test/data/diffs/full/csharp-valvesoftware-openvr-add-many-constants-and-fields/after.cs.test — Model, Unknown, Count, Auto
- src/test/data/diffs/full/csharp-valvesoftware-openvr-add-many-constants-and-fields/before.cs.test — Model, Unknown, Count, Auto

## Tests
### research/tests/test_analysis.py
- latex number uses the papers thousands separator
- read rows keeps every value as a string
- read rows with fields returns the header in file order
- read rows with fields on an empty file
- repo root is the directory holding research
- bucket index puts each loc in the first bucket whose bound exceeds it
- bucket label formats the open top bucket with a plus
- tool names come from the mismatch columns in order
- ms values splits the semicolon joined repeats
- applicable rows drops fixtures the tool did not score
- common subset keeps only fixtures every tool scored
- pct is zero where the total is zero
- percentile by nearest rank
- shallow boundary commits reads the graft points
- shallow boundary commits is empty for a complete clone
- expand substitutes matrix and env expressions
- expand treats a missing env value as empty and a missing matrix value as an error
- expand refuses expressions it cannot evaluate
- matrix combinations is the cartesian product of the list axes
- matrix combinations expands a standalone include one entry per job
- matrix combinations merges an include into the combinations it agrees with
- matrix combinations adds an include that matches no combination
- matrix combinations applies a keyless include to every combination
- matrix combinations refuses exclude rather than ignoring it
- area of picks the most specific prefix
- area of puts a module root with its own module
- area of gives top level files their own area not other
- badge color follows the shields thresholds
- the ruff version named outside ci matches the one ci pins
- percentile of counts agrees with the list percentile
- ecdf is monotone and ends at one hundred
- rq1 series keeps timed out pairs in the denominator
- distribution rows are the whole population as value counts
- rename sides recovers both paths of a numstat rename
- per test coverage classifies module roots with their modules
- coverage sets groups a fixtures tests but leaves other tests alone
- coverage sets masks to the area and combines sets
- coverage sets refuses bits that do not match their record
### src/bin/analyze_human_mappings.rs
- sibling candidate rejects depth parent and kind changes
- renumbering after a deletion is not a reorder
- swapping two siblings is a reorder
- reorder inversions are counted per parent and kind
### src/bin/benchmark_astdiff_oracle.rs
- resolve skips a leading javadoc
- resolve tolerates a trailing semicolon or colon
- resolve widens arguments to their parentheses
- resolve rejects a span no tolerance explains
- by span lists the outermost node first
- byte offset converts utf16 units past non ascii text
- statement level keeps parameters but not declaration fragments
- a pair under a byte identical element is excluded
### src/bin/benchmark_diff_pairs.rs
- measures a real pair from handmade repository
- skips pairs above the node ceiling
- measures a fixture directory
- paper fixture dirs include fixtures without ground truth
### src/bin/benchmark_optimal_solutions.rs
- non apted reason labels covers every bucket label
- apted is deliberately not a fixed column
- a fixture that got worse fails the gate
- a fixture whose mismatches became visible fails the gate
- improvements pass and are reported separately
- a new hard fixture is reported but does not fail the gate
- a run that lost most of the corpus fails even with no regression
- dropping a single fixture still passes
- a fixture missing from the run is named rather than silently ignored
- a text only fixture stays out of the baseline without counting as unsolved
- an unsolved fixture is counted but never enters the baseline
- latency is reported but never fails the gate
- a freshly written baseline passes against its own run
- a baseline name with stray whitespace still gates its fixture
- a baseline without elapsed ms reads as untimed
### src/bin/benchmark_other.rs
- gumtree node offsets parses the trailing bracketed range
- gumtree node offsets errors without a bracketed suffix
- gumtree line range finds the single line a small range sits on
- gumtree line range spans every line a range crosses
- gumtree line range does not pull in the next line when end lands on a boundary
- gumtree line range handles multi byte utf8 text before the touched range
- gumtree line range clamps an end offset past the end of the file
- char offset table maps char offsets to rows and byte columns
- span from char offsets clamps past the end instead of panicking
- merge spans coalesces adjacent and overlapping spans but keeps gaps
- mean coefficient of variation ignores single sample fixtures
- merge spans handles the empty case
- git line labels treats a zero count side as untouched
- git myers agrees with unix diff
- bdiff spans from script reads str diff as inclusive character offsets
- bdiff spans from script skips an empty side range
- bdiff spans from script falls back to whole lines without str diff
- span on row chars converts character offsets to byte columns
- bdiff touched from script follows the documented mode rules
- bdiff touched from script rejects an unknown mode
### src/bin/benchmark_other/diffsitter.rs
- diffsitter touched from json reads old and new hunks as zero indexed lines
### src/bin/benchmark_other/difftastic.rs
- difftastic touched from json marks a side with empty changes as touched
- difftastic touched from json without chunks touches nothing
### src/bin/benchmark_other/gumtree.rs
- gumtree node offsets reads the final range when the text contains brackets
- gumtree touched from json marks moves on the dest side through matches
### src/bin/commit_stats.rs
- end to end
### src/bin/diff_inventory.rs
- painting state distinguishes the three states the painter cares about
- every fixture in the corpus produces a row
- a handmade fixture has blank provenance rather than a missing row
- error nodes and their percentage come from the real parse
- missing nodes are not counted as errors
### src/bin/fake_diff_tool.rs
- empty mode omits the chunks key entirely
- all mode covers every line including the one after the final newline
- all mode reports byte columns not character offsets
- random mode does not collapse onto the parity of its input
- random mode is a pure function of side index and line
- random mode spans start and end on character boundaries
- an unknown mode is rejected
### src/bin/file_stats.rs
- end to end
- min bytes filters before anything is read
- small batch size still writes everything
- stops early once db size cap is reached
- rerunning against the same db updates rather than duplicates
### src/bin/generate_mapping_site.rs
- render code panel reproduces the source text exactly
- render code row paints only the changed columns
- render code row does not wrap a middle rows trailing whitespace
- render code row draws a caret for the other sides deletion
- code markers clamps an end of file caret onto the last row
- render code panel folds a long unchanged run but renders a short one
- code visible rows keeps context around a change and drops the rest
- anchor rows anchors on a caret with no changed row
- code visible rows shows everything when nothing anchors
- a paintings anchors are unioned with the tree panels own
- code counterparts links real text directly and a deletion to its caret
- painting panels give every span of one match a single shared id
- painting panels draw no caret for a one sided entry
- each rendering gets its own dom id prefix
- an unreadable painting is reported and skipped rather than failing the page
- a painted fixture page stacks one code panel per painting
- escape html text escapes the three html metacharacters but not quotes
- escape html attr also escapes double quotes
- render fixture page links to the fixtures directory in this repo
- render fixture page links to the upstream commit when there is one
- render fixture page has no upstream link without provenance
- path for node agrees with viewer js on a shared example
- render node emits a leaf div and a nonleaf details with nested children
- render node marks matched nodes with a data match pointing at the other side
- render node marks a non identical matched node as changed with its own operation class
- render node marks an identical but relocated match as moved
- render node names every counterpart of an all to all member and badges it
- render node badges an any one to one member without changing its match
- extents cover reads a range against the union of a groups members
- share all to all ids gives a groups moved and updated spans one id per side
- a page with an all to all group links the copies to the original in the code view
- render node marks a deleted node without a data match
- fully quiet subtree sizes treats matched as quiet but deleted as not
- fully quiet subtree sizes excludes a non identical match even though its matched
- render node keeps the root open even when the whole tree is fully quiet
- render node omits a large fully unmarked subtree behind a placeholder
- render node omits a large fully matched subtree but keeps its own data match
- render node closes but still fully renders a small fully quiet subtree
- render index page links to each fixtures page and shows its language
- render index page puts each mismatch count in its own sortable column
- render index page escapes fixture names
### src/bin/generate_showcase.rs
- every case names a fixture that exists and each group has ten
- case names are unique
- unix ranges pair untouched lines and run touched ones together
- painted counts take each two sided operation once
- unix ranges of a whole file rewrite is one range per side
- unix ranges of identical files are all identical pairs
- baking every case matches the published scores
### src/bin/human_solver/tests.rs
- validate new case name rejects empty
- validate new case name rejects leading digit
- validate new case name rejects unsafe characters
- validate new case name accepts letters digits hyphen underscore
- validate new case name rejects rust keywords
- stub test contents has no comment block when none given
- stub test contents has no comment block when comment is empty or whitespace
- stub test contents includes a wrapped comment block right before the assert
- wrap comment lines keeps a short comment on one line
- wrap comment lines wraps long comments at word boundaries
- wrap comment lines never splits a single word even if it exceeds the width
- is state preserving key is true only for typing in the three text input modals
- is state preserving key is false for enter esc on the same three modals
- is state preserving key is false for every other modal and for no modal at all
- is state preserving key is true for pure navigation and display keys with no modal open
- is state preserving key is false for keys that can mutate mapping or collapse state
- count unmarked counts only nodes with no match or delete mark
- render panel only scans the visible window not the whole flat list
- sample source deserializes a legacy source json missing dataset as small
- sample source deserializes an explicit dataset field
- default promoted name lowercases language and strips dot git
- default promoted name leaves a repository with no dot git suffix alone
- default promoted name for path lowercases the detected language
- default promoted name for path is empty for an undetected language
- promote target dataset is none for diffs and handmade for a git commit file
- promote target dataset is the samples own recorded dataset
- action reject bails when current case is not a sample
- action reject bails on an empty reason
- short hash takes the first eight characters
- short hash does not panic on a shorter input
- advance to next search match finds the next leaf containing the query and wraps around
- advance to next search match only matches leaf nodes not a containers concatenated text
- advance to next search match returns none and leaves the cursor put when nothing matches
- action search reports the matched nodes kind and moves the focused panels cursor
- action search errors when nothing in the focused panel matches
- handle modal key prompt search enter finds a match and remembers the query
- handle modal key prompt search enter on empty input cancels without touching last search
- handle modal key prompt search esc cancels without searching
- handle modal key prompt search backspace and char edit the input and keep the modal open
- render modal prompt search shows the prefilled query and instructions
- render modal prompt promote name shows the actual target dataset not a fixed one
- run unix diff reports no differences for identical content
- run unix diff shows added and removed lines
- raw before after reads the before and after test files from a directory
- raw before after is none when a file is missing
- sample diff line count counts only changed lines not context or headers
- sample diff line count is zero for a nonexistent sample
- sample diff line count is nonzero for a real sample on disk
- visible diff options can narrow to painted or unpainted cases
- filters on different columns combine as an and
- visible diff options sorts by the selected column with a name tiebreak
- visible diff options sorts and filters by diff size
- changed line count reads a case directory like a sample
- visible diff options narrows by a name substring
- the solution picker lists existing paintings before suggestions
- e on a diff opens the comment prompt
- e on an un noted diff opens an empty prompt
- e on a git commit case refuses with a message
- no promoted row carries a comment
- deleting a painting takes two presses of d
- deleting the last painting leaves the fixture unpainted
- moving the cursor cancels a pending deletion
- deleting another painting leaves you editing your own
- d on an unused suggestion deletes nothing
- branching keeps both paintings on file
- branching minimal to full widens a wholly changed line to its indentation
- the two branched paintings satisfy their own preset rules
- branching leaves a partly changed line alone
- branching leaves a matched line alone
- branching to a free form name widens nothing
- branching empty starts the new painting from nothing
- branching to an existing name switches without overwriting it
- loading switches which painting is edited without touching any
- ranges painted under one name stay out of another
- the text overlay cycles human codediff disagreements tree disagreement
- codediff text spans reports the changed regions of a real diff
- the disagreement overlay is empty when the two accounts match
- painted ranges use the shared overlay palette
- span covers stops at the last real character of a middle row
- span covers still covers a blank middle row
- span covers uses the exact end column on the last row
- the palette falls back to the default theme when none was installed
- a selection uses the cross panel highlight colour
- x banks ranges so m can commit an n to m match
- the live selection commits together with banked ranges
- m refuses a group whose spans differ within a side
- d paints every banked range as one entry
- x in the text view banks the live selection
- c in the text view clears both sides banks
- text diff hunks finds one entry per differing region
- n in the text view jumps both sides to the next hunk
- p in the text view walks back and wraps to the last hunk
- the focused side decides which hunk is next
- n on an unchanged pair reports no differences
- a in the text view aligns the other side on the same line
- a clamps to the other sides last line
- a minimal full line sweep is painted without any indentation
- the split painting passes invariant 6 and the unsplit one does not
- a full painting keeps the sweep exactly as drawn
- a vertical selection is never reshaped
- a minimal sweep commits over a range that only overlapped the indentation
- a minimal sweep over blank lines only paints nothing and says so
- skip leading whitespace splits a sweep row by row
- caret moves to the first non whitespace character
- the first code column is a byte offset
- the navigation keys work in text only mode
- o cycles the overlay and p no longer does
- the unix diff view numbers both sides
- colon jumps to a line in the text view
- text view keys do not invalidate the frame state
- a fresh painting test passes and says it means nothing yet
- a fresh invariants test is generated strict with no number to fill in
- a selection includes the character under the cursor
- a backwards selection normalizes to the same span
- a selection past a multibyte character lands on the right text
- a multi row selection is a stack of per row spans not a line sweep
- a multi row selection skips a row shorter than the selected columns
- toggling off vertical restores the full line sweep
- vertical selection leaves a middle rows tail unstyled but full line does not
- stepping across a multibyte character lands on boundaries
- m pairs both sides selections and derives move from identical text
- m without a selection on both sides paints nothing and says so
- d and i paint one sided ranges on their own side
- u removes a whole match from either side
- z marks an unpainted fixture as deliberately empty
- z refuses to touch a fixture that already has painted ranges
- the text view renders painted ranges
- render open diff picker shows the unmarked column and the sort and filter markers
- the diff picker title keeps its filter list when the terminal truncates it
- text view modal renders both sides content
- unix diff modal renders diff output
- centered rect at least uses the percentage when it already meets the minimum
- centered rect at least grows past the percentage to meet the minimum
- centered rect at least never exceeds the available area
- render text modal shows every line including the input box on a small terminal
- open sample picker renders every column including the bucket
- visible sample rows sorts by the selected column
- bucket order ranks by the lower bound not the label
- the bucket filter never hides a sample with no recorded stratum
- sample filters narrow together rather than either or
- the size filter finds samples whose diff is empty
- the name filter is a case insensitive substring
- value filters cycle through the values present and back to all
- the status filter cycles through all three states and back
- the sample picker title names the sorted column and the filters
- open sample picker enter opens the visible entry not the raw index
- open sample picker s sorts by the cursor column and keeps the selected row
- open sample picker f persists the column filter on app
- open commit picker j k move selection clamped to bounds
- open commit picker esc cancels
- open commit picker enter on an unresolvable commit reports an error without crashing
- open commit file picker enter opens the selected file as an open target
- open commit file picker enter from a dirty git commit file case cannot save directly
- open commit file picker esc cancels
- open sample picker modal selects the currently open case under the given view
- open sample picker modal falls back to the first entry when the current case is not a sample
- visible diff options narrows to the given dataset
- visible diff options cmpl filter excludes only cases the map measured
- the title lists a matching cmpl and unmarked filter once
- the cmpl and unmarked filters select the same rows
- next dataset filter cycles through diff datasets and back to all
- open diff picker modal selects the currently open case under the given filter
- open diff picker modal falls back to the first entry when the current case is filtered out
- open diff picker h and l move the column cursor and clamp at the ends
- diff picker invariant column separates unscanned from clean
- open diff picker f on the dataset column persists the filter on app
- open diff picker s sorts by the cursor column and flips on a second press
- open diff picker name filter prompt swallows command keys until enter
- open diff picker name filter prompt clears on an empty submission
- open diff picker column movement does not trigger a corpus scan
- draw ui shows only the focused panel below the single panel width threshold
- help modal renders keybindings
- fully solved nodes hides fully marked subtree but keeps unmarked ancestors
- count unmarked nodes in tree counts every hole not just the first
- scan corpus returns the same map at every worker count
- scan corpus handles more workers than entries
- scan corpus propagates a worker panic
- default scan threads is at least one and within the cap
- diff case unmarked count returns some for a real case on disk
- open diff picker f on cmpl uses the cached unmarked map without recomputing it
- open diff picker f computes the unmarked map lazily when not yet cached
- flatten visible skips hidden subtree entirely but keeps siblings
- fully solved nodes hides a subtree matched for real via m
- action match to end matches identical trees completely
- action match to end stops at a kind mismatch but keeps prior matches
- action match to end focuses after when the diff only adds
- one sided diff names a side only when the diff has one
- action match to end does not pair a trailing statement against the wrong node
- action match to end is linear not quadratic in tree size
- action match subtree is linear not quadratic in tree size
- m preserves a pre existing match under a subtree it bails out of
- m replaces a pre existing match on a node it actually revisits
- update sample csv sets promoted to on the matching row only
- update sample csv returns false when no row matches
- update sample csv returns false when file does not exist
- sample triage statuses at reads the status column for every row
- sample metadata at defaults an unmatched row to sampled
- sample metadata at is empty when file does not exist
- update sample csv sets status to promoted
- reject sample csv at sets reason and status without touching promoted to
- reject sample csv at returns false when no row matches
- set sample comment at sets comment without touching status or promoted to
- set sample comment at with an empty comment clears a previous one
- set sample comment at returns false when no row matches
- sample comment at returns the trimmed comment when present
- sample comment at is none when comment is empty or row is missing
- algo reason reports the pass that produced each side of a match
- algo reason is none when the diff has no entry for the node
- reason label matches benchmark optimal solutions abbreviations
- reason detail shows apted provenance but reason label does not
- multi map group operation is identical only when every member shares one hash
- commit multi map group replaces any prior entry touching its nodes
- commit multi map group replaces a prior group sharing a node
- commit multi map group orders paths by source position not by arena id
- commit multi map group with children clears a pre existing descendant entry
- action commit multi map group errors when a member is under a deleted with children ancestor
- action commit multi map group errors when one side is empty
- action commit multi map group raises a modal for mixed kinds
- action commit multi map group commits directly when kinds match
- action unmark on a group member removes the whole group
- handle key x toggles multi select and c clears both sides
- handle key m with a pending selection commits a multi map group
- handle key capital x flips the pending selections pairing and says so
- handle key m with an all to all selection commits it and resets the pairing
- handle key c drops the pairing with the selection
- confirming a mixed kind all to all group records the pairing it was raised with
- action unmark names the kind of group it removes
- all to all subtree groups returns one member set per position
- all to all subtree groups keeps every member of an n to m selection together
- all to all subtree groups reports a kind divergence and returns nothing
- all to all subtree groups reports a child count divergence
- handle key capital m on an all to all selection commits every position
- handle key lowercase m on an all to all selection still commits only the roots
- handle key capital m commits nothing when the subtrees diverge
- render panel marks an all to all member with a capital g
- render panel marks a group matched node and a pending selection distinctly
- text view renders no literal tabs for a tab indented fixture
- text view keeps one screen column per source character
- display safe str replaces every tab and leaves everything else
- the text view never puts a carriage return in the buffer
- a crlf row ends at its last visible character
- display safe leaves multi byte control code points alone
- sample csv round trip preserves the size bucket
- round tripping the real sample csv loses nothing
- codediff text entries keeps the pairing that the span view drops
- seeding a painting reproduces codediffs own spans on both sides
- seeding refuses to overwrite a painting that already has ranges
- seeding refuses a pair whose codediff ranges overlap
- spans overlap detects a shared byte and allows touching ranges
- a long line wraps across screen rows with a blank continuation gutter
- wrapping never emits more screen rows than the viewport holds
- the cursor row stays visible when the rows above it wrap
- a width with no room beside the gutter does not wrap or hang
- a painted span keeps its style across a wrap boundary
- wrapping measures terminal cells not characters
- painting over an already painted range is refused
- painting two ranges that meet at a newline is allowed
- resetting a case clears the mapping the groups and every painting
- resetting a case needs the explicit key and enter will not do
- compute frame state has no roots for a pair with no grammar
- draw ui names the missing grammar instead of drawing an empty tree
- the paint view opens without a tree
- a tree key explains itself in text only mode
- the same key is not explained away when there is a tree
- codediff text spans falls back to the plain text diff
- a text only stub has no mapping test
- a text only fixture file carries a painting and invariants but no mapping
- a in the text view puts the tree cursor on the leaf under it
- a in the text view moves the panel for the side it is on
- a in the text view lands on the next leaf from inter token whitespace
- a in the text view finds the right leaf in a crlf file
- byte offset counts a crlf terminator as two bytes
- a in the text view says so on a side with no syntax tree
- v lists the violations of the in memory mapping
- v says so when a case breaks nothing
- enter in the invariant list moves the tree and the text cursor
- algo disagrees never flags a node the human has not marked
- algo disagrees flags a match to a different partner
- advance to next mismatch wraps around in both directions
- advance to next mismatch leaves the cursor put when nothing disagrees
- reveal node expands collapsed ancestors and centers an offscreen target
- reveal node moves nothing for an id not in the tree
- insert use line leaves a file that already has the import untouched
- delete with children drops a prior match on a descendant
- update sample csv clears the rows comment
- action promote refuses a sample whose recorded dataset is unknown
- esc in the text view clears the selection then the bank before closing
- esc while naming a painting returns to the picker list
- the paint column counts a deliberately empty painting as painted
- first leaf from skips whitespace to the next leaf and is none past the last one
### src/bin/materialize_test_diffs.rs
- base name matches language x repository commit filename scheme
- materializes a real pair from handmade repository
- rerun is idempotent and distinct content gets a numbered suffix
- rerun keeps a hand edited readme and backfills a missing one
### src/bin/sample_code_pairs.rs
- samples real pairs from handmade repository
### src/bin/sample_test_diffs.rs
- samples real pairs from handmade repository
- stratified sampling records a size bucket per row
- stratified top up ignores unstratified rows and counts by bucket
- tops up existing samples without duplicates
- dataset defaults to the repos dir parent name
- stratified defaults to the stratified dataset and rejects any other
- rows without dataset or status columns get their legacy defaults
### src/code.rs
- is binary file agrees with from file on valid utf8
- is binary file agrees with from file on invalid utf8
- is binary file says dev null is not binary
- is binary file says valid utf8 containing a nul byte is not binary
- code from empty string
- code from string skips ast metadata when the language has no grammar
- cloning code drops ast metadata so its ids cannot outlive the tree
- code from file
- parse code
- ast metadata computed in from string
- ast metadata computed in from file
- ast metadata consistency
- ensure parsed already parsed and metadata set
- ensure parsed parsed but no metadata
- ensure parsed not parsed
- ensure parsed no language
- ensure parsed unsupported language
### src/code/gap_survey.rs
- gap survey (skipped)
### src/code/hash.rs
- hash all handmade codes
- full vs structural hashing
- full hash ignores reindentation
- full hash counts string content a grammar leaves in a gap
- kind and value hash of an ancestor survives reordering a commutative container
- identical code produces same hashes
- different code structural similarity
- benchmark function works
### src/code/language.rs
- language for invalid extensions
- language for valid extensions
- language for path strips test suffix
- ts extension with xml content is qt linguist not typescript
- ts extension with ts content is still typescript
- xml content sniff is scoped to ts extension
### src/code/metadata.rs
- compute ast metadata does not panic when language is unset
- hermetic expand from path
- compute row byte lengths empty string
- compute row byte lengths single line
- compute row byte lengths single line with newline
- compute row byte lengths multiple lines
- compute row byte lengths varying lengths
- compute row byte lengths with empty lines
- compute row byte lengths multibyte characters
- compute row byte lengths agrees with byte offsets on mixed rows
- compute ast metadata works
### src/code/similarity.rs
- identical sets score one and disjoint sets score zero
- small sets are exact not estimated
- one changed leaf out of many stays near one
- merge is order independent
- merging is associative over intermediate nodes
- saturated sketches estimate large set similarity
### src/code/similarity_corpus_tests.rs
- sketch recovers a permutation of near identical yaml urls
- sketch grades the near miss the equality hashes call different
### src/code/tip.rs
- type from extension for invalid extensions
- type from extension for valid extensions
- grammar backed extensions are code before any table
- ambiguous extensions stay unclassified
- man page sections
- web platform test header sidecars are text
- filename wins over extension
- filename families by prefix
- dotfiles without an extension
- extension matching is case insensitive
### src/configure_prompt.rs
- shell quote leaves a plain path alone
- shell quote single quotes a path with spaces or shell syntax
- shell quote escapes an embedded single quote
- resolve codediff path never returns an empty string
- parse yes no accepts y n and their full spellings case insensitively
- parse yes no falls back to the default on an empty line
- parse yes no rejects anything else
### src/diff.rs
- diff code does not panic when language is unknown
- pending finish does not panic when language is unknown
- rust completely unrelated main files resolves fast
- compute metadata
- diff empty rust code
- diff identical rust code
- diff hello world with translated string
- identical code must always match
- hello world translations in all languages
- is valid with identical code
- is valid with different code
- is valid with invalid mapping
- is valid with null mapping
- add mapping updates all maps
- diff populates all maps
- remove match mapping leaves both nodes undecided
- is complete does not require the roots to be mapped
### src/diff/apted/common/slots.rs
- a surviving call keeps its own closing paren when a nested call is removed
- a separator is left where the dp put it
### src/diff/apted/common/tests.rs
- apted engine forced right matches oracle fuzz
- apted engine matches oracle single leaf
- apted engine matches oracle small trees
- apted engine matches oracle multi root forest
- apted engine matches oracle deep unbalanced
- bench compute delta large balanced trees (skipped)
- bench compute delta typical random trees (skipped)
- apted engine matches oracle fuzz minimal repro
- apted engine matches oracle tiny repro
- debug dump tiny repro (skipped)
- debug dump n7 repro (skipped)
- debug dump minimal repro (skipped)
- shrink apted engine fuzz failure (skipped)
- apted engine matches oracle fuzz
- apted engine matches oracle fuzz with containment
- apted engine matches oracle with pruned descendants
- already matched nodes are skipped
- honors pre existing match and still finds nested reuse
- no change
- hello world added message
- hello world removed message
- python added if block small
- python added if block
- rust add if
- flat tree myers diff matches changed tokens
- myers lcs basic
- myers lcs identical
- myers lcs empty
- myers lcs exceeds limit
- maximal unmatched roots stops at first unmatched node each branch
- resolve residual forest via myers lcs matches identical and recurses the rest
- resolve residual forest via myers lcs does not relabel across different kinds
- resolve residual forest via myers lcs replaces everything past the edit cap
- ren charges for text a node owns directly
- ren never pairs a worded comment with a bare marker
- ren keeps leaf updates strictly cheaper than delete plus insert
- apted whole tree hands an oversized pair to the kernel instead of decomposing it
- split into anchored segments splits at matched children and drops them
- split into anchored segments ignores an anchor whose partner is behind the last split
- anchor leftovers by member name zips equal counts and leaves unequal counts alone
- widest statement sequence body takes the shallowest body over a wider nested one
- prematch identical statement siblings maps only identical statements
- mutual similarity pairs a rewrite and leaves the surplus as inserts
- mutual similarity refuses a tie between two candidates
- mutual similarity pairs nothing unless the smaller side is fully paired
- containment forbids pairing a hollowed out ancestor away from its pruned descendant
- longest increasing by second drops pairs that contradict the longest ordered run
- slot lcs anchor outweighs every promotion it blocks
- for roots and the fallback are no ops without an ast
### src/diff/cost.rs
- identical and unchanged match but not identical cost nothing
- single node operations cost one regardless of subtree size
- with children operations scale by subtree size
- diff cost sums single node entries without double counting
- diff cost scales with children entries by metadata subtree size
- match but not identical charges for a node s own changed text
- diff cost charges a gap owning matched pair
### src/diff/grouped_greedy_matcher.rs
- only same key candidates are ever paired
- cheapest pair in a group wins and each side is claimed once
- max cost rejects pairs above the threshold
- none max cost accepts regardless of cost
- already mapped before candidates are skipped
- already mapped after candidates are skipped
- ties are broken by input order for determinism
- a candidate mapped by an earlier on accept is not paired again
### src/diff/hash_tree_matching.rs
- build extended node list always includes reference nodes regardless of size
- build extended node list excludes small non reference nodes
- build extended node list includes non reference nodes above the size threshold
- build extended node list sorts by subtree size descending
- pair children for descent zips positionally and drops mismatched kinds
- pair children for descent detects reordering in a commutative container
- pair children for descent reports no reorder when nothing moved
- pair children for descent pairs reordered children by value not by position
- pair children for descent breaks ties by sibling index not byte offset
### src/diff/nodes/tests.rs
- structural visibility does not depend on what the file is diffed against
- structural visibility excludes containers and keeps text carriers
- operator family masks agree with string scanning kinds update allowed
- kinds update allowed same kind is always allowed
- kinds update allowed cross kind identifiers
- kinds update allowed identifiers do not match non identifiers
- flow control similarity of sets ignores wildcards and scores jaccard
- flow control similarity of sets is zero when either side is empty
- cpp relational operators cross match
- cpp operators never cross families
- cpp increment decrement cross match
- rust range operators cross match
- rust compound assignment crosses with plain assignment
- unknown language never allows cross kind matches
- python excludes keyword comparisons from family
- generic token kind covers operators and punctuation
- generic token kind excludes content bearing leaves
- matching allowed rejects whatever kinds update allowed rejects
- matching allowed requires context for generic tokens only
- leaf texts similar accepts clear renames
- leaf texts similar rejects unrelated and tiny texts
- root nodes are reference in all languages
- rust public items are matched
- root nodes are not semantically structural in all languages
- go functions methods and types are matched
- go subtest run calls are matched by their literal name
- go calls that are not literal named subtests are not matched
- go top level var and const declarations are matched
- python functions and classes are matched
- python methods in class are pre matched
- rust traits and modules are matched
- go type alias is matched
- kotlin top level functions are matched
- kotlin extension functions use receiver prefix
- kotlin classes objects and aliases are matched
- kotlin unnamed companion object is not matched
- kotlin named companion object is matched
- kotlin methods in class are pre matched
- rust bail macro and ordinary call are classified correctly
- c fprintf to stderr is diagnostic
- python logging error is diagnostic via attribute access
- go log fatal is diagnostic via selector expression
- rust recognizes struct fields enum variants and use lists
- go recognizes struct fields and import specs
- python recognizes dictionary
- java recognizes enum body
- csharp recognizes enum member declaration list
- c and cpp recognize enumerator list
- js ts tsx recognize object
- scala recognizes braced import selectors
- swift recognizes enum class body
- kotlin has no commutative container
- json and yaml recognize objects and both mapping shapes
- cpp googletest blocks are keyed by suite and case
- cpp function named test with named parameters keeps its own name
- js top level const is keyed only when single declarator and top level
- collect unmatched skips mapped subtrees but descends into collected nodes
- map identical descendants leaves an already mapped child and its subtree alone
### src/diff/solve_bottom_up_propagation.rs
- renamed function with identical body matches via propagation
- disagreeing after parents block the match
- parent of only deleted children is deleted with children
- parent with an undecided child is left undecided
### src/diff/solve_greedy_anchor_blocks.rs
- identical children sequence costs nothing
- one changed statement only costs that statement
- anonymous if block with mostly identical body is anchored
- completely different blocks are not anchored
- blocks in unrelated structural positions are not anchored even with similar content
### src/diff/solve_hash_descent.rs
- reordered commutative container is distinguished from truly identical
### src/diff/solve_heritage_clause_growth.rs
- class body pushed by a new implements clause is tagged
- interface body pushed by a new extends clause is tagged
- a body whose content also changed is left alone
- a body that did not shift keeps its reason
### src/diff/solve_identical_diagnostic_statements.rs
- identical bail macro is matched across renamed functions
- changed diagnostic statement is not matched
- non diagnostic identical call is not matched
- duplicate identical diagnostic calls are matched one to one
### src/diff/solve_large_flat_subtrees.rs
- large flat macro body is myers diffed
- small macro body is left alone
- large flat top level json object is myers diffed
- small json object is left alone
- data literal table is myers diffed even when not the widest subtree
- named subtest inside a data literal function is prematched by name
- kind unique top level wrapper reaches a nested flat literal
- repeated top level wrapper kind has no identity
### src/diff/solve_leading_siblings.rs
- is comment function
- matches leading comment of an unchanged sibling function
- matches leading attribute of an unchanged sibling mod item
- matches a chain of two leading attributes
- does not match a changed leading attribute
- leading sibling chain stops at first text mismatch and keeps earlier hops
### src/diff/solve_leaf_neighbour_agreement.rs
- an operator chain that grew keeps each existing token where it was
- an invocation chain that grew keeps its existing dot
- a comma is not re pointed when an argument was inserted between its neighbours
- a leaf with only one matched neighbour is left alone
- a claimed target is never taken
- an identical leaf between matched neighbours is re pointed to their middle
- a non identical leaf pairing is never re pointed
### src/diff/solve_moved_subtrees.rs
- moved function is matched not deleted
- tiny identical statements do not move
- ambiguous small moves are refused rather than guessed
- context tiebreak picks the candidate in the more familiar surroundings
- context tiebreak refuses when the surroundings are equally alike
- context tiebreak refuses a near tie
- context tiebreak declines to rank a crowd of commodity tokens
### src/diff/solve_mutual_ancestors.rs
- a container whose content moved up one level is matched
- a container holding foreign matched content is not paired
- ancestors above an unwrapped list are matched end to end
### src/diff/solve_nested_condition_collapse.rs
- outer if and its condition are matched across a let chain collapse
- a lone if let is left alone
- a chain with an else branch is left alone
- a chain whose condition changed is left alone
### src/diff/solve_orphaned_leaves.rs
- a dropped comma between the same arguments is paired
- unequal orphan counts pair nothing
- a comma between different arguments is not paired
### src/diff/solve_syntax_aware_matching.rs
- methods in different impls are matched within their own impl
- go subtests named by literal are individually matched via qualified name
- overloaded same name functions are matched nm
- grouped use statement survives symbol set churn
- grouped use statements with no symbol overlap are not matched
- import path moved into a subdirectory is the same module
- short import paths differing in two tokens are different modules
- long import paths get a larger differing token budget
- import paths sharing one token are different modules
- import pair with a rival on either side is refused
### src/diff/solve_unique_type_matching.rs
- unique leftover child kind matches under an already matched parent
- ambiguous multiple candidates of the same kind do not match
### src/diff/solve_unresolved_nodes.rs
- wrapper nodes left undecided by matching get explicit inserts
- both roots are paired rather than deleted and inserted
- every node of both trees ends up with a decision
### src/diff/solve_wrap_growth.rs
- an existing if else becoming an else if branch is tagged
- a run of statements wrapped in a new try block are all tagged
- only byte identical content is ever tagged even when a sibling condition changed
- a sibling shift at the same level is not a wrap
- top level statements wrapped in a new try are tagged
### src/diff/text/tests.rs
- neither preset turns on whole pair updates
- minimal and full disagree on paint reindent only moves
- paint reindent only moves gates only tagged nodes
- minimal and full disagree on paint displaced moves
- paint displaced moves gates a node pushed down by an insertion above it
- paint displaced moves gates a multi row node edited on its first row
- reconcile moves keeps two overlapping accounts of one relocation
- reconcile moves believes the side that blames fewer rows
- reconcile moves breaks a tie in favour of the before side
- minimal and full disagree on paint resized moves
- only the post filters skip a rebuild
- minimal and full disagree on whole identifier updates
- minimal drops the punctuation the painted corpus drops
- minimal keeps operators and anything carrying meaning
- an empty range is not structural
- full returns every range unchanged
- minimal drops a standalone bracket but keeps its neighbour
- minimal keeps a range that merely contains punctuation
- minimal keeps identical ranges even when they are pure punctuation
- minimal trims leading and trailing whitespace off a range
- minimal keeps whitespace inside a range
- full keeps leading but still trims trailing whitespace
- leading whitespace off splits a multiline insert per row trimmed
- leading whitespace off drops a blank interior row
- leading whitespace off does not split a multiline update
- leading whitespace off still drops a structural only row
- structural punctuation off alone still keeps leading whitespace
- leading whitespace off alone still keeps a range containing punctuation
- a lone closing paren is restored when its open partner survives
- a pair that is entirely standalone punctuation stays dropped
- a lone move bracket is not restored
- trimming leaves the destination alone
- a multi row range with content below a blank first row survives
- a multi row range of only whitespace is dropped
- minimal keeps a range it cannot read
- plain text line diff matches identical lines
- plain text line diff finds a pure insertion
- plain text line diff finds a pure deletion
- plain text line diff treats a dissimilar changed line as delete plus insert
- plain text line diff narrows a changed line to the changed part
- plain text line diff keeps a block insert merged
- plain text line diff resynchronises after an inserted line
- plain text line diff intra line columns are byte offsets
- plain text line diff handles non contiguous matches
- plain text line diff anchors unmatched runs at the preceding matchs destination
- plain text line diff handles empty before as a pure insertion
- plain text line diff handles empty after as a pure deletion
- plain text line diff treats two empty files as no changes
- plain text line diff replaces the whole file past the edit cap
- plain text line diff handles a ten thousand line file with scattered changes
- whole file class identical when no lines changed
- whole file class insert only when nothing deleted
- whole file class delete only when nothing inserted
- whole file class mixed when both inserted and deleted
- whole file class mixed when myers lcs gives up
- whole file text class matches independent census (skipped)
- line operations does not let a same row identical range hide a real change
- line operations treats a zero width range as a placeholder not a real row
- change counts tallies insertions deletions and updates without double counting
- change counts tallies moves once from the after side
- no change all ranges
- hello world added message all ranges
- python leetcode 1 added if block all ranges
- whitespace stripped equal ignores all whitespace differences
- summarize diff is no changes when every range is identical
- summarize diff is no changes for two empty files
- summarize diff is new file when only inserts are present
- summarize diff is not new file when inserts are mixed with identical content
- summarize diff is not deleted file when deletes are mixed with identical content
- summarize diff is deleted file when only deletes are present
- summarize diff is whitespace only when stripped content matches despite move ranges
- summarize diff is refactor moved only when only moves are present and content really differs
- summarize diff is none for a genuine mixed edit
- summarize diff prefers whitespace only over refactor when both could apply
- summarize diff is whitespace only even with zero operations when content is not byte identical
- ranges paints a wholly new comments own words not just its marker
- a no gap string literal does not break whitespace merging with a sibling
- sibling reorder produces move ranges and a refactor moved summary
- unrelated insertion does not flag shifted content as moved
- is comment only diff is true when only a comments text changed
- is comment only diff is true when a comment was inserted
- is comment only diff is true when a comment was deleted
- is comment only diff is false for a real code change
- is comment only diff is false when a comment and real code both changed
- is comment only diff is false when nothing changed at all
- is comment only diff is false when a statement moves one level deeper
- summarize diff with comment check reports comment only over no classification
- summarize diff with comment check reports comment only over refactor moved only
- summarize diff with comment check does not override new file
- summarize diff with comment check ignores the flag when false
- ranges decomposes a small change inside a long identifier
- full paints a renamed identifier whole
- full keeps a changed comment narrow
- ranges reports the whole identifier when whole pair updates is set
- ranges falls back to a whole span update when there is no common affix
- ranges decomposes a small change inside a comment
- ranges decomposition survives an unrelated earlier insertion
- a phrase added inside a string renders as an insert not an update
- a renamed identifier stays an update even though one side is empty
- paint displaced moves gates a node between two edits on its own row
- a row rewritten around its one surviving fragment still paints move
- a node whose text repeats on its row is not read as displaced
- common affixes never split a multibyte character
- plain text line diff never reports a move
- shared affix declines two identical lines
- trailing trim keeps a range that runs off a file with no final newline
- full grows an insert over the indentation of its own line
- full does not grow an insert over indentation a matched range shares
- bracket pair partners skips a mismatched bracket
- toggling an out of range option is a no op
### src/diff/text_range.rs
- byte index agrees with a linear walk on the corpus (skipped)
- byte index agrees with a linear walk everywhere
- is zero recognizes only the origin sentinel while is empty holds anywhere
- columns on row spans a middle row whole and skips rows outside
- text range intersects overlapping
- text range intersects touching
- text range intersects identical
- text range intersects contains
- text range intersects disjoint
- text range intersects same row different columns
- text range intersects same row touching columns
- text range intersects empty range
- text range intersects empty ranges same point
- text range intersects crossing rows
- from treesitter range does not normalize a mid row column that matches a character count
- from treesitter range end at line end
- from treesitter range end not at line end
- from treesitter range multiline
- from treesitter range end at last line end
- from treesitter range empty range
- from treesitter range end row beyond columns
- from treesitter range end row beyond columns nonzero column
- text range can extend exact touch
- text range can extend with whitespace only
- text range cannot extend with non whitespace
- text range can extend same line with spaces
- text range cannot extend same line with text
- text range cannot extend multi line with non whitespace
- row col to byte index lands after a multi byte character
- text range can extend with whitespace after a multi byte character earlier in the line
- floor char boundary clamps to the line and rounds down inside a character
- paint row len stops before trailing whitespace
- row len is bytes not characters
- screen column counts cells not bytes or characters
- screen column rounds down inside a multi byte character
- screen column is monotonic across every byte column of a mixed row
- corpus ranges are addressable byte columns sharing nothing but line terminators
### src/git_configure.rs
- difftool command quotes a path with spaces and keeps gits variables bare
- scope flag is always explicit
- parse scope accepts g l and their full spellings case insensitively
- parse scope defaults to global on an empty line
- parse scope rejects anything else
### src/jj_configure.rs
- scope flags match jjs own spelling
- parse scope accepts u r and their full spellings case insensitively
- parse scope defaults to user on an empty line
- parse scope rejects anything else
### src/lib.rs
- diff empty strings
### src/main.rs
- resolve before after with no args starts an empty viewer
- resolve before after with two args is before after directly
- resolve before after with seven args picks out old file and new file
- resolve before after accepts gits nine argument rename form
- resolve before after still rejects an unrecognized argument count
- resolve before after with seven args passes dev null through for add delete
- whole updates flag layers onto minimal and full alike
- paint reindent moves flag layers onto minimal and full alike
- color flag parses all three choices and defaults to auto
- context flag defaults to the headless default and accepts overrides
- mode accepts its three values case insensitively and nothing else
- tui rates reject zero negative and non numbers
- tui rates are hidden from help
- no color opts out only when set and non empty
- the no files message names the missing files when text mode was asked for
- a broken pipe anywhere in the chain is recognized
- always and never beat the no color environment variable
- without the flag a differing pair still exits zero
- with the flag exit codes follow the diff convention
- the git external diff form never returns one even with the flag
- a binary pair under the git external diff form still exits zero
- the binary notice names gits logical path not its temp blobs
- the binary notice names both sides for the two argument form
- the binary notice does not claim identical files differ
- exit code flag defaults to off and parses
- git configure parses as a subcommand
- two paths still parse as paths not a subcommand
- util generates completions and a man page
- the help text describes codediff rather than a doc comment
- no args still opens empty viewer
- should run headless when stdout is not a terminal even without any flag
- should run headless is false on a real terminal with no flags
- should run headless honors the headless flag even on a real terminal
- batch flag is a clap alias for headless
- should run headless honors mode headless case insensitively
- should run json honors mode json case insensitively
- should run json is false on a non terminal stdout unless explicitly asked for
- resolve before after rejects any other count
### src/review.rs
- name status parses plain and two path records
- log records split on the unit separator
- change set labels are footer sized
- load lists the three sets of a real repository
- an empty repository lists nothing and does not fail
- outside a repository is an error naming the directory
- materializing each kind of change keeps the real path and the right contents
- a missing blob is a clear error not a panic
### src/stats.rs
- detects generated files
- deeply nested trees are walked without recursion
- kind stats bucket subtree sizes by log2
- counts lines correctly
### src/stats/filesystem.rs
- walks directory and skips anomalous paths
- for each repository visits every path with its derived name
- for each repository keeps going after one repository errors
- single file path is returned directly
- single anomalous file path is skipped
### src/stats/license.rs
- classifies common license texts
- blob url builds a commit pinned link per host
- render readme without license files warns explicitly
- render readme links to the license file instead of embedding its text
- render readme lists the filename without a link for an unrecognized host
- render readme with an unverifiable reason explains why instead of claiming no license
### src/stats/sampling.rs
- reservoir never exceeds capacity
- reservoir keeps everything below capacity
- loc bucket boundaries
### src/test.rs
- the clamped stubs explain their limits
- every fixture stub is declared in its dataset module
- stub mapping limits reads both call shapes and skips the hand written stub
- the quality baseline accuracy columns are a projection of the stub limits
- handmade test code loads
- handmade git repository loads
### src/test/data/diffs/full/rust-fornwall-rust-script-add-lifecycle-management/after.rs.test
- split input
- find embedded manifest
- scrape markdown manifest
- extract comment
### src/test/data/diffs/full/rust-fornwall-rust-script-add-lifecycle-management/before.rs.test
- split input
- find embedded manifest
- scrape markdown manifest
- extract comment
### src/test/data/diffs/full/rust-rbspy-rbspy-add-two-test-cases/after.rs.test
- get ruby stack trace 1 9 3
- get ruby stack trace 2 1 6
- get ruby stack trace 2 1 6 2
- get ruby stack trace 2 4 0
- get ruby stack trace 2 5 0
- get ruby stack trace 2 7 2
- get ruby stack trace 2 7 3
- get ruby stack trace 2 7 4
- get ruby stack trace 2 7 5
- get ruby stack trace 2 7 6
- get ruby stack trace 2 7 7
- get ruby stack trace 2 7 8
- get ruby stack trace 3 0 0
- get ruby stack trace 3 0 1
- get ruby stack trace 3 0 2
- get ruby stack trace 3 0 3
- get ruby stack trace 3 0 4
- get ruby stack trace 3 0 5
- get ruby stack trace 3 0 6
- get ruby stack trace 3 0 7
- get ruby stack trace 3 1 0
- get ruby stack trace 3 1 1
- get ruby stack trace 3 1 2
- get ruby stack trace 3 1 3
- get ruby stack trace 3 1 4
- get ruby stack trace 3 1 5
- get ruby stack trace 3 1 6
- get ruby stack trace 3 1 7
- get ruby stack trace 3 2 0
- get ruby stack trace 3 2 1
- get ruby stack trace 3 2 2
- get ruby stack trace 3 2 3
- get ruby stack trace 3 2 4
- get ruby stack trace 3 2 5
- get ruby stack trace 3 2 6
- get ruby stack trace 3 2 7
- get ruby stack trace 3 2 8
- get ruby stack trace 3 3 0
- get ruby stack trace with classes 3 3 0
- get ruby stack trace 3 3 1
- get ruby stack trace 3 3 2
- get ruby stack trace 3 3 3
- get ruby stack trace 3 3 4
- get ruby stack trace 3 3 5
- get ruby stack trace 3 3 6
- get ruby stack trace 3 3 7
- get ruby stack trace 3 3 8
- get ruby stack trace 3 3 9
- get ruby stack trace 3 3 10
- get ruby stack trace 3 4 0
- get ruby stack trace 3 4 1
- get ruby stack trace 3 4 2
- get ruby stack trace 3 4 3
- get ruby stack trace 3 4 4
- get ruby stack trace 3 4 5
- get ruby stack trace complex 3 4 5
- get ruby stack trace 3 4 6
- get ruby stack trace 3 4 7
### src/test/data/diffs/full/rust-rbspy-rbspy-add-two-test-cases/before.rs.test
- get ruby stack trace 1 9 3
- get ruby stack trace 2 1 6
- get ruby stack trace 2 1 6 2
- get ruby stack trace 2 4 0
- get ruby stack trace 2 5 0
- get ruby stack trace 2 7 2
- get ruby stack trace 2 7 3
- get ruby stack trace 2 7 4
- get ruby stack trace 2 7 5
- get ruby stack trace 2 7 6
- get ruby stack trace 2 7 7
- get ruby stack trace 2 7 8
- get ruby stack trace 3 0 0
- get ruby stack trace 3 0 1
- get ruby stack trace 3 0 2
- get ruby stack trace 3 0 3
- get ruby stack trace 3 0 4
- get ruby stack trace 3 0 5
- get ruby stack trace 3 0 6
- get ruby stack trace 3 0 7
- get ruby stack trace 3 1 0
- get ruby stack trace 3 1 1
- get ruby stack trace 3 1 2
- get ruby stack trace 3 1 3
- get ruby stack trace 3 1 4
- get ruby stack trace 3 1 5
- get ruby stack trace 3 1 6
- get ruby stack trace 3 1 7
- get ruby stack trace 3 2 0
- get ruby stack trace 3 2 1
- get ruby stack trace 3 2 2
- get ruby stack trace 3 2 3
- get ruby stack trace 3 2 4
- get ruby stack trace 3 2 5
- get ruby stack trace 3 2 6
- get ruby stack trace 3 2 7
- get ruby stack trace 3 2 8
- get ruby stack trace 3 3 0
- get ruby stack trace with classes 3 3 0
- get ruby stack trace 3 3 1
- get ruby stack trace 3 3 2
- get ruby stack trace 3 3 3
- get ruby stack trace 3 3 4
- get ruby stack trace 3 3 5
- get ruby stack trace 3 3 6
- get ruby stack trace 3 3 7
- get ruby stack trace 3 3 8
- get ruby stack trace 3 4 0
- get ruby stack trace 3 4 1
- get ruby stack trace 3 4 2
- get ruby stack trace 3 4 3
- get ruby stack trace 3 4 4
- get ruby stack trace 3 4 5
- get ruby stack trace complex 3 4 5
- get ruby stack trace 3 4 6
- get ruby stack trace 3 4 7
### src/test/data/diffs/full/tsx-greenbone-gsa-add-import-and-use-it/after.tsx.test
- ScannerComponent tests › should allow to clone a scanner
- ScannerComponent tests › should handle error on cloning a scanner
- ScannerComponent tests › should allow to delete a scanner
- ScannerComponent tests › should handle error on deleting a scanner
- ScannerComponent tests › should allow to edit a scanner
- ScannerComponent tests › should allow to create a new scanner
- ScannerComponent tests › should handle error on creating a new scanner
### src/test/data/diffs/full/tsx-greenbone-gsa-add-import-and-use-it/before.tsx.test
- ScannerComponent tests › should allow to clone a scanner
- ScannerComponent tests › should handle error on cloning a scanner
- ScannerComponent tests › should allow to delete a scanner
- ScannerComponent tests › should handle error on deleting a scanner
- ScannerComponent tests › should allow to edit a scanner
- ScannerComponent tests › should allow to create a new scanner
- ScannerComponent tests › should handle error on creating a new scanner
### src/test/data/diffs/full/tsx-popcorn-official-popcorn-desktop-simple-property-rename/after.tsx.test
- SplashRoute › shows Splash while boot is not initialized
- SplashRoute › redirects to onboarding when not onboarded
- SplashRoute › redirects to login when onboarded but session is not active
- SplashRoute › shows splash when active but app not initialized
- SplashRoute › redirects to browser when active and app initialized
- SplashRoute › reacts when app initialization flips
### src/test/data/diffs/full/tsx-popcorn-official-popcorn-desktop-simple-property-rename/before.tsx.test
- SplashRoute › shows Splash while boot is not initialized
- SplashRoute › redirects to onboarding when not onboarded
- SplashRoute › redirects to login when onboarded but session is not active
- SplashRoute › shows splash when active but app not initialized
- SplashRoute › redirects to browser when active and app initialized
- SplashRoute › reacts when app initialization flips
### src/test/data/diffs/handmade/rust-add-comments-and-real-new-logic/after.rs.test
- samples real pairs from handmade repository
- tops up existing samples without duplicates
### src/test/data/diffs/handmade/rust-add-comments-and-real-new-logic/before.rs.test
- samples real pairs from handmade repository
- tops up existing samples without duplicates
### src/test/data/diffs/handmade/rust-adding-a-variable-and-test-with-comments/after.rs.test
- render code panel reproduces the source text exactly
- render code row paints only the changed columns
- render code row does not wrap a middle rows trailing whitespace
- render code row draws a caret for the other sides deletion
- code visible rows keeps context around a change and drops the rest
- anchor rows anchors on a caret with no changed row
- code visible rows shows everything when nothing anchors
- a paintings anchors are unioned with the tree panels own
- code counterparts links real text directly and a deletion to its caret
- painting panels give every span of one match a single shared id
- painting panels draw no caret for a one sided entry
- each rendering gets its own dom id prefix
- an unreadable painting is reported and skipped rather than failing the page
- a painted fixture page stacks one code panel per painting
- escape html text escapes the three html metacharacters but not quotes
- escape html attr also escapes double quotes
- render fixture page links to the fixtures directory in this repo
- render fixture page links to the upstream commit when there is one
- render fixture page has no upstream link without provenance
- path for node agrees with viewer js on a shared example
- render node emits a leaf div and a nonleaf details with nested children
- render node marks matched nodes with a data match pointing at the other side
- render node marks a non identical matched node as changed with its own operation class
- render node marks an identical but relocated match as moved
- render node marks a deleted node without a data match
- fully quiet subtree sizes treats matched as quiet but deleted as not
- fully quiet subtree sizes excludes a non identical match even though its matched
- render node keeps the root open even when the whole tree is fully quiet
- render node omits a large fully unmarked subtree behind a placeholder
- render node omits a large fully matched subtree but keeps its own data match
- render node closes but still fully renders a small fully quiet subtree
- render index page links to each fixtures page and shows its language
- render index page puts each mismatch count in its own sortable column
- render index page escapes fixture names
### src/test/data/diffs/handmade/rust-adding-a-variable-and-test-with-comments/before.rs.test
- render code panel reproduces the source text exactly
- render code row paints only the changed columns
- render code row draws a caret for the other sides deletion
- code visible rows keeps context around a change and drops the rest
- anchor rows anchors on a caret with no changed row
- code visible rows shows everything when nothing anchors
- a paintings anchors are unioned with the tree panels own
- code counterparts links real text directly and a deletion to its caret
- painting panels give every span of one match a single shared id
- painting panels draw no caret for a one sided entry
- each rendering gets its own dom id prefix
- an unreadable painting is reported and skipped rather than failing the page
- a painted fixture page stacks one code panel per painting
- escape html text escapes the three html metacharacters but not quotes
- escape html attr also escapes double quotes
- render fixture page links to the fixtures directory in this repo
- render fixture page links to the upstream commit when there is one
- render fixture page has no upstream link without provenance
- path for node agrees with viewer js on a shared example
- render node emits a leaf div and a nonleaf details with nested children
- render node marks matched nodes with a data match pointing at the other side
- render node marks a non identical matched node as changed with its own operation class
- render node marks an identical but relocated match as moved
- render node marks a deleted node without a data match
- fully quiet subtree sizes treats matched as quiet but deleted as not
- fully quiet subtree sizes excludes a non identical match even though its matched
- render node keeps the root open even when the whole tree is fully quiet
- render node omits a large fully unmarked subtree behind a placeholder
- render node omits a large fully matched subtree but keeps its own data match
- render node closes but still fully renders a small fully quiet subtree
- render index page links to each fixtures page and shows its language
- render index page puts each mismatch count in its own sortable column
- render index page escapes fixture names
### src/test/data/diffs/handmade/rust-completely-unrelated-main-files/after.rs.test
- merge scan targets dedupes and sorts
- merge scan targets with no leases returns just devs
- merge scan targets with empty devs still returns leases
### src/test/data/diffs/handmade/rust-completely-unrelated-main-files/before.rs.test
- resolve before after with no args starts an empty viewer
- resolve before after with two args is before after directly
- resolve before after with seven args picks out old file and new file
- resolve before after with seven args passes dev null through for add delete
- should run headless when stdout is not a terminal even without any flag
- should run headless is false on a real terminal with no flags
- should run headless honors the headless flag even on a real terminal
- should run headless honors mode headless case insensitively
- resolve before after rejects any other count
### src/test/data/diffs/handmade/rust-firefox-webrenderer-borders/after.rs.test
- struct sizes
### src/test/data/diffs/handmade/rust-firefox-webrenderer-borders/before.rs.test
- struct sizes
### src/test/data/diffs/handmade/rust-real-logic-change-in-a-huge-75k-node-file/after.rs.test
- validate new case name rejects empty
- validate new case name rejects leading digit
- validate new case name rejects unsafe characters
- validate new case name accepts letters digits hyphen underscore
- validate new case name rejects rust keywords
- is state preserving key is true only for typing in the three text input modals
- is state preserving key is false for enter esc on the same three modals
- is state preserving key is false for every other modal and for no modal at all
- count unmarked counts only nodes with no match or delete mark
- render panel only scans the visible window not the whole flat list
- sample source deserializes a legacy source json missing dataset as small
- sample source deserializes an explicit dataset field
- default promoted name lowercases language and strips dot git
- default promoted name leaves a repository with no dot git suffix alone
- default promoted name for path lowercases the detected language
- default promoted name for path is empty for an undetected language
- promote target dataset is none for diffs and handmade for a git commit file
- promote target dataset is the samples own recorded dataset
- action reject bails when current case is not a sample
- action reject bails on an empty reason
- short hash takes the first eight characters
- short hash does not panic on a shorter input
- advance to next search match finds the next leaf containing the query and wraps around
- advance to next search match only matches leaf nodes not a containers concatenated text
- advance to next search match returns none and leaves the cursor put when nothing matches
- action search reports the matched nodes kind and moves the focused panels cursor
- action search errors when nothing in the focused panel matches
- handle modal key prompt search enter finds a match and remembers the query
- handle modal key prompt search enter on empty input cancels without touching last search
- handle modal key prompt search esc cancels without searching
- handle modal key prompt search backspace and char edit the input and keep the modal open
- render modal prompt search shows the prefilled query and instructions
- render modal prompt promote name shows the actual target dataset not a fixed one
- run unix diff reports no differences for identical content
- run unix diff shows added and removed lines
- raw before after reads the before and after test files from a directory
- raw before after is none when a file is missing
- sample diff line count counts only changed lines not context or headers
- sample diff line count is zero for a nonexistent sample
- sample diff line count is nonzero for a real sample on disk
- text view modal renders both sides content
- unix diff modal renders diff output
- centered rect at least uses the percentage when it already meets the minimum
- centered rect at least grows past the percentage to meet the minimum
- centered rect at least never exceeds the available area
- render text modal shows every line including the input box on a small terminal
- open sample picker marks solved and rejected entries and can hide both
- render open sample picker shows the current sort order
- visible sample options orders by the requested sort order
- sample sort order next cycles through all four and back
- open sample picker enter opens the visible entry not the raw index
- open sample picker s advances sort order and resets selection to first
- open sample picker h persists hide solved on app
- open commit picker j k move selection clamped to bounds
- open commit picker esc cancels
- open commit picker enter on an unresolvable commit reports an error without crashing
- open commit file picker enter opens the selected file as an open target
- open commit file picker enter from a dirty git commit file case cannot save directly
- open commit file picker esc cancels
- open sample picker modal selects the currently open case under the given sort order
- open sample picker modal falls back to the first entry when the current case is not a sample
- visible diff options narrows to the given dataset
- visible diff options hide complete excludes only cases the map marks complete
- next dataset filter cycles through diff datasets and back to all
- open diff picker modal selects the currently open case under the given filter
- open diff picker modal falls back to the first entry when the current case is filtered out
- open diff picker d persists dataset filter on app
- draw ui shows only the focused panel below the single panel width threshold
- help modal renders keybindings
- fully solved nodes hides fully marked subtree but keeps unmarked ancestors
- tree has unmarked node is false only once every node is marked
- diff case is incomplete returns some for a real case on disk
- open diff picker h toggles hide complete using the cached completeness map
- open diff picker h computes completeness lazily when not yet cached
- flatten visible skips hidden subtree entirely but keeps siblings
- fully solved nodes hides a subtree matched for real via m
- action match to end matches identical trees completely
- action match to end stops at a kind mismatch but keeps prior matches
- action match to end does not pair a trailing statement against the wrong node
- action match to end is linear not quadratic in tree size
- action match subtree is linear not quadratic in tree size
- m preserves a pre existing match under a subtree it bails out of
- m replaces a pre existing match on a node it actually revisits
- update sample csv sets promoted to on the matching row only
- update sample csv returns false when no row matches
- update sample csv returns false when file does not exist
- sample triage statuses at reads the status column for every row
- sample triage statuses at defaults an unmatched row to sampled
- sample triage statuses at is empty when file does not exist
- update sample csv sets status to promoted
- reject sample csv at sets reason and status without touching promoted to
- reject sample csv at returns false when no row matches
- algo reason reports the pass that produced each side of a match
- algo reason is none when the diff has no entry for the node
- reason label matches benchmark optimal solutions abbreviations
- reason detail shows apted provenance but reason label does not
- multi map group operation is identical only when every member shares one hash
- commit multi map group replaces any prior entry touching its nodes
- commit multi map group replaces a prior group sharing a node
- commit multi map group orders paths by source position not by arena id
- commit multi map group with children clears a pre existing descendant entry
- action commit multi map group errors when a member is under a deleted with children ancestor
- action commit multi map group errors when one side is empty
- action commit multi map group raises a modal for mixed kinds
- action commit multi map group commits directly when kinds match
- action unmark on a group member removes the whole group
- handle key x toggles multi select and c clears both sides
- handle key m with a pending selection commits a multi map group
- render panel marks a group matched node and a pending selection distinctly
### src/test/data/diffs/handmade/rust-real-logic-change-in-a-huge-75k-node-file/before.rs.test
- validate new case name rejects empty
- validate new case name rejects leading digit
- validate new case name rejects unsafe characters
- validate new case name accepts letters digits hyphen underscore
- validate new case name rejects rust keywords
- is state preserving key is true only for typing in the two text input modals
- is state preserving key is false for enter esc on the same two modals
- is state preserving key is false for every other modal and for no modal at all
- count unmarked counts only nodes with no match or delete mark
- render panel only scans the visible window not the whole flat list
- sample source deserializes a legacy source json missing dataset as small
- sample source deserializes an explicit dataset field
- default promoted name lowercases language and strips dot git
- default promoted name leaves a repository with no dot git suffix alone
- default promoted name for path lowercases the detected language
- default promoted name for path is empty for an undetected language
- promote target dataset is none for diffs and handmade for a git commit file
- promote target dataset is the samples own recorded dataset
- short hash takes the first eight characters
- short hash does not panic on a shorter input
- advance to next search match finds the next leaf containing the query and wraps around
- advance to next search match only matches leaf nodes not a containers concatenated text
- advance to next search match returns none and leaves the cursor put when nothing matches
- action search reports the matched nodes kind and moves the focused panels cursor
- action search errors when nothing in the focused panel matches
- handle modal key prompt search enter finds a match and remembers the query
- handle modal key prompt search enter on empty input cancels without touching last search
- handle modal key prompt search esc cancels without searching
- handle modal key prompt search backspace and char edit the input and keep the modal open
- render modal prompt search shows the prefilled query and instructions
- render modal prompt promote name shows the actual target dataset not a fixed one
- run unix diff reports no differences for identical content
- run unix diff shows added and removed lines
- raw before after reads the before and after test files from a directory
- raw before after is none when a file is missing
- sample diff line count counts only changed lines not context or headers
- sample diff line count is zero for a nonexistent sample
- sample diff line count is nonzero for a real sample on disk
- text view modal renders both sides content
- unix diff modal renders diff output
- centered rect at least uses the percentage when it already meets the minimum
- centered rect at least grows past the percentage to meet the minimum
- centered rect at least never exceeds the available area
- render text modal shows every line including the input box on a small terminal
- open sample picker marks solved entries and can hide them
- render open sample picker shows the current sort order
- visible sample options orders by the requested sort order
- sample sort order next cycles through all four and back
- open sample picker enter opens the visible entry not the raw index
- open sample picker s advances sort order and resets selection to first
- open sample picker h persists hide solved on app
- open commit picker j k move selection clamped to bounds
- open commit picker esc cancels
- open commit picker enter on an unresolvable commit reports an error without crashing
- open commit file picker enter opens the selected file as an open target
- open commit file picker enter from a dirty git commit file case cannot save directly
- open commit file picker esc cancels
- open sample picker modal selects the currently open case under the given sort order
- open sample picker modal falls back to the first entry when the current case is not a sample
- visible diff options narrows to the given dataset
- visible diff options hide complete excludes only cases the map marks complete
- next dataset filter cycles through diff datasets and back to all
- open diff picker modal selects the currently open case under the given filter
- open diff picker modal falls back to the first entry when the current case is filtered out
- open diff picker d persists dataset filter on app
- draw ui shows only the focused panel below the single panel width threshold
- help modal renders keybindings
- fully solved nodes hides fully marked subtree but keeps unmarked ancestors
- tree has unmarked node is false only once every node is marked
- diff case is incomplete returns some for a real case on disk
- open diff picker h toggles hide complete using the cached completeness map
- open diff picker h computes completeness lazily when not yet cached
- flatten visible skips hidden subtree entirely but keeps siblings
- fully solved nodes hides a subtree matched for real via m
- action match to end matches identical trees completely
- action match to end stops at a kind mismatch but keeps prior matches
- action match to end does not pair a trailing statement against the wrong node
- action match to end is linear not quadratic in tree size
- action match subtree is linear not quadratic in tree size
- m preserves a pre existing match under a subtree it bails out of
- m replaces a pre existing match on a node it actually revisits
- update sample csv sets promoted to on the matching row only
- update sample csv returns false when no row matches
- update sample csv returns false when file does not exist
- promoted sample sources at only includes rows with a non empty promoted to
- promoted sample sources at is empty when file does not exist
- algo reason reports the pass that produced each side of a match
- algo reason is none when the diff has no entry for the node
- reason label matches benchmark optimal solutions abbreviations
- reason detail shows apted provenance but reason label does not
- multi map group operation is identical only when every member shares one hash
- commit multi map group replaces any prior entry touching its nodes
- commit multi map group replaces a prior group sharing a node
- commit multi map group orders paths by source position not by arena id
- commit multi map group with children clears a pre existing descendant entry
- action commit multi map group errors when a member is under a deleted with children ancestor
- action commit multi map group errors when one side is empty
- action commit multi map group raises a modal for mixed kinds
- action commit multi map group commits directly when kinds match
- action unmark on a group member removes the whole group
- handle key x toggles multi select and c clears both sides
- handle key m with a pending selection commits a multi map group
- render panel marks a group matched node and a pending selection distinctly
### src/test/data/diffs/handmade/rust-small-addition-with-reuse-of-binary-expressions/after.rs.test
- minimal drops the punctuation the painted corpus drops
- minimal keeps operators and anything carrying meaning
- an empty range is not structural
- full returns every range unchanged
- minimal drops a standalone bracket but keeps its neighbour
- minimal keeps a range that merely contains punctuation
- minimal keeps identical ranges even when they are pure punctuation
- minimal keeps a range it cannot read
- plain text line diff matches identical lines
- plain text line diff finds a pure insertion
- plain text line diff finds a pure deletion
- plain text line diff treats a dissimilar changed line as delete plus insert
- plain text line diff narrows a changed line to the changed part
- plain text line diff keeps a block insert merged
- plain text line diff resynchronises after an inserted line
- plain text line diff intra line columns are byte offsets
- plain text line diff handles non contiguous matches
- plain text line diff anchors unmatched runs at the preceding matchs destination
- plain text line diff handles empty before as a pure insertion
- plain text line diff handles empty after as a pure deletion
- plain text line diff treats two empty files as no changes
- plain text line diff replaces the whole file past the edit cap
- plain text line diff handles a ten thousand line file with scattered changes
- whole file class identical when no lines changed
- whole file class insert only when nothing deleted
- whole file class delete only when nothing inserted
- whole file class mixed when both inserted and deleted
- whole file class mixed when myers lcs gives up
- whole file text class matches independent census (skipped)
- line operations does not let a same row identical range hide a real change
- line operations treats a zero width range as a placeholder not a real row
- change counts tallies insertions deletions and updates without double counting
- change counts tallies moves once from the after side
- no change all ranges
- hello world added message all ranges
- python leetcode 1 added if block all ranges
- whitespace stripped equal ignores all whitespace differences
- summarize diff is no changes when every range is identical
- summarize diff is no changes for two empty files
- summarize diff is new file when only inserts are present
- summarize diff is not new file when inserts are mixed with identical content
- summarize diff is not deleted file when deletes are mixed with identical content
- summarize diff is deleted file when only deletes are present
- summarize diff is whitespace only when stripped content matches despite move ranges
- summarize diff is refactor moved only when only moves are present and content really differs
- summarize diff is none for a genuine mixed edit
- summarize diff prefers whitespace only over refactor when both could apply
- summarize diff is whitespace only even with zero operations when content is not byte identical
- sibling reorder produces move ranges and a refactor moved summary
- unrelated insertion does not flag shifted content as moved
- is comment only diff is true when only a comments text changed
- is comment only diff is true when a comment was inserted
- is comment only diff is true when a comment was deleted
- is comment only diff is false for a real code change
- is comment only diff is false when a comment and real code both changed
- is comment only diff is false when nothing changed at all
- is comment only diff is false when a statement moves one level deeper
- summarize diff with comment check reports comment only over no classification
- summarize diff with comment check reports comment only over refactor moved only
- summarize diff with comment check does not override new file
- summarize diff with comment check ignores the flag when false
- ranges decomposes a small change inside a long identifier
- ranges falls back to a whole span update when there is no common affix
- ranges decomposes a small change inside a comment
- ranges decomposition survives an unrelated earlier insertion
### src/test/data/diffs/handmade/rust-small-addition-with-reuse-of-binary-expressions/before.rs.test
- minimal drops the punctuation the painted corpus drops
- minimal keeps operators and anything carrying meaning
- an empty range is not structural
- full returns every range unchanged
- minimal drops a standalone bracket but keeps its neighbour
- minimal keeps a range that merely contains punctuation
- minimal keeps identical ranges even when they are pure punctuation
- minimal keeps a range it cannot read
- plain text line diff matches identical lines
- plain text line diff finds a pure insertion
- plain text line diff finds a pure deletion
- plain text line diff treats a dissimilar changed line as delete plus insert
- plain text line diff narrows a changed line to the changed part
- plain text line diff keeps a block insert merged
- plain text line diff resynchronises after an inserted line
- plain text line diff intra line columns are byte offsets
- plain text line diff handles non contiguous matches
- plain text line diff anchors unmatched runs at the preceding matchs destination
- plain text line diff handles empty before as a pure insertion
- plain text line diff handles empty after as a pure deletion
- plain text line diff treats two empty files as no changes
- plain text line diff replaces the whole file past the edit cap
- plain text line diff handles a ten thousand line file with scattered changes
- whole file class identical when no lines changed
- whole file class insert only when nothing deleted
- whole file class delete only when nothing inserted
- whole file class mixed when both inserted and deleted
- whole file class mixed when myers lcs gives up
- whole file text class matches independent census (skipped)
- line operations does not let a same row identical range hide a real change
- line operations treats a zero width range as a placeholder not a real row
- change counts tallies insertions deletions and updates without double counting
- change counts tallies moves once from the after side
- no change all ranges
- hello world added message all ranges
- python leetcode 1 added if block all ranges
- whitespace stripped equal ignores all whitespace differences
- summarize diff is no changes when every range is identical
- summarize diff is no changes for two empty files
- summarize diff is new file when only inserts are present
- summarize diff is not new file when inserts are mixed with identical content
- summarize diff is not deleted file when deletes are mixed with identical content
- summarize diff is deleted file when only deletes are present
- summarize diff is whitespace only when stripped content matches despite move ranges
- summarize diff is refactor moved only when only moves are present and content really differs
- summarize diff is none for a genuine mixed edit
- summarize diff prefers whitespace only over refactor when both could apply
- summarize diff is whitespace only even with zero operations when content is not byte identical
- sibling reorder produces move ranges and a refactor moved summary
- unrelated insertion does not flag shifted content as moved
- is comment only diff is true when only a comments text changed
- is comment only diff is true when a comment was inserted
- is comment only diff is true when a comment was deleted
- is comment only diff is false for a real code change
- is comment only diff is false when a comment and real code both changed
- is comment only diff is false when nothing changed at all
- is comment only diff is false when a statement moves one level deeper
- summarize diff with comment check reports comment only over no classification
- summarize diff with comment check reports comment only over refactor moved only
- summarize diff with comment check does not override new file
- summarize diff with comment check ignores the flag when false
- ranges decomposes a small change inside a long identifier
- ranges falls back to a whole span update when there is no common affix
- ranges decomposes a small change inside a comment
- ranges decomposition survives an unrelated earlier insertion
### src/test/data/diffs/handmade/rust-sniffnet-protocol/after.rs.test
- protocol display
- all protocols collection
### src/test/data/diffs/handmade/rust-sniffnet-protocol/before.rs.test
- protocol display
- all protocols collection
### src/test/data/diffs/handmade/rust-zed-workspace-tasks/after.rs.test
- schedule resolved task save all
- schedule resolved task save current
- schedule resolved task save none
### src/test/data/diffs/small/rust-rustdesk-rustdesk-large-file-40k-normal-feature-work/after.rs.test
- retina
### src/test/data/diffs/small/rust-rustdesk-rustdesk-large-file-40k-normal-feature-work/before.rs.test
- retina
### src/test/data/diffs/small/tsx-apache-superset-add-test-case/after.tsx.test
- should render
- should render the navigation
- should render the environment tag
- should render all the top navbar menu items
- should render the top navbar child menu items
- should render the dropdown items
- should render the Settings
- should render the Settings menu item
- should render the Settings dropdown child menu items
- should render the plus menu (+) when user is not anonymous
- should NOT render the plus menu (+) when user is anonymous
- should render the user actions when user is not anonymous
- should NOT render the user actions when user is anonymous
- should render the About section and version_string, sha or build_number when available
- should render the Documentation link when available
- should render the Bug Report link when available
- should render the Login link when user is anonymous
- should render the Language Picker
- should hide create button without proper roles
- should render without QueryParamProvider
- should render an extension component if one is supplied
- should render the brand text if available
- should not render the brand text if not available
- brand logo href should not be prefixed with app root when brandLogoHref is an absolute URL
- brand logo href should not be prefixed with app root when brandLogoHref is protocol-relative
- brand path should be prefixed with app root in subdirectory deployment
- brand link falls back to brand.path when theme brandLogoUrl is absent
### src/test/data/diffs/small/tsx-apache-superset-add-test-case/before.tsx.test
- should render
- should render the navigation
- should render the environment tag
- should render all the top navbar menu items
- should render the top navbar child menu items
- should render the dropdown items
- should render the Settings
- should render the Settings menu item
- should render the Settings dropdown child menu items
- should render the plus menu (+) when user is not anonymous
- should NOT render the plus menu (+) when user is anonymous
- should render the user actions when user is not anonymous
- should NOT render the user actions when user is anonymous
- should render the About section and version_string, sha or build_number when available
- should render the Documentation link when available
- should render the Bug Report link when available
- should render the Login link when user is anonymous
- should render the Language Picker
- should hide create button without proper roles
- should render without QueryParamProvider
- should render an extension component if one is supplied
- should render the brand text if available
- should not render the brand text if not available
- brand logo href should not be prefixed with app root when brandLogoHref is an absolute URL
- brand logo href should not be prefixed with app root when brandLogoHref is protocol-relative
- brand path should be prefixed with app root in subdirectory deployment
- brand link falls back to brand.path when theme brandLogoUrl is absent
### src/test/data/diffs/small/tsx-material-remove-import/after.tsx.test
- VariantColorProvider › should provide default variant and color
- VariantColorProvider › variant `solid` should inherit variant and color
- VariantColorProvider › variant `soft` should inherit variant and color
- VariantColorProvider › variant `outlined` should set variant to plain and color to neutral
- VariantColorProvider › variant `plain` should set color to neutral
- VariantColorProvider › should use instance variant and color
- VariantColorProvider › should use default variant and color
### src/test/data/diffs/small/tsx-material-remove-import/before.tsx.test
- VariantColorProvider › should provide default variant and color
- VariantColorProvider › variant `solid` should inherit variant and color
- VariantColorProvider › variant `soft` should inherit variant and color
- VariantColorProvider › variant `outlined` should set variant to plain and color to neutral
- VariantColorProvider › variant `plain` should set color to neutral
- VariantColorProvider › should use instance variant and color
- VariantColorProvider › should use default variant and color
### src/test/data/diffs/small/typescript-microsoft-playwright-add-two-test-cases/after.ts.test
- expectVisible not found error
- expectVisible not visible error
- not expectVisible visible error
- expectChecked not checked error
- expectValue wrong value error
- expectAria wrong snapshot error
- expect timeout during run
- expect timeout during generate
- expectURL success
- expectURL wrong URL error
- expectURL with regex
- expectURL with regex error
- expectTitle success
- expectTitle wrong title error
### src/test/data/diffs/small/typescript-microsoft-playwright-add-two-test-cases/before.ts.test
- expectVisible not found error
- expectVisible not visible error
- not expectVisible visible error
- expectChecked not checked error
- expectValue wrong value error
- expectAria wrong snapshot error
- expect timeout during run
- expect timeout during generate
- expectURL success
- expectURL wrong URL error
- expectURL with regex
- expectURL with regex error
### src/test/data/diffs/small/typescript-microsoft-typescript-add-dot-js-to-import-paths/after.ts.test
- transpile test ${this.justName} has expected ${kind === TranspileKind.Module ? "js" : "declaration"} output
### src/test/data/diffs/small/typescript-microsoft-typescript-add-dot-js-to-import-paths/before.ts.test
- transpile test ${this.justName} has expected ${kind === TranspileKind.Module ? "js" : "declaration"} output
### src/test/data/diffs/stratified/rust-gyulyvgc-sniffnet-rename-one-identifier/after.rs.test
- mac simple test
- mac all zero test
- ipv6 simple test
- ipv6 zeros in the middle
- ipv6 leading zeros
- ipv6 tail one after zeros
- ipv6 tail zeros
- ipv6 multiple zero sequences first longer
- ipv6 multiple zero sequences first longer head
- ipv6 multiple zero sequences second longer
- ipv6 multiple zero sequences second longer tail
- ipv6 multiple zero sequences equal length
- ipv6 all zeros
- ipv6 x all zeros
- ipv6 all zeros x
- ipv6 many zeros but no compression
- traffic direction ipv4 test
- traffic type multicast ipv4 test
- traffic type multicast ipv6 test
- traffic type host local broadcast test
- traffic type host directed broadcast test
- is local connection ipv4 test
- is local connection ipv6 test
- is local connection ipv4 2 test
- is local connection ipv4 multicast test
- is local connection ipv6 multicast test
- is local connection ipv4 link local test
- is local connection ipv6 link local test
- get service simple only one valid
- get service well known ports always win
- get service direction bonus matters
- get service multicast bonus matters
- get service broadcast bonus matters
- get service different tcp udp
- get service not applicable
- get service unknown
- all services map key and values are valid
- service names of old application protocols
- other service names
### src/test/data/diffs/stratified/rust-gyulyvgc-sniffnet-rename-one-identifier/before.rs.test
- mac simple test
- mac all zero test
- ipv6 simple test
- ipv6 zeros in the middle
- ipv6 leading zeros
- ipv6 tail one after zeros
- ipv6 tail zeros
- ipv6 multiple zero sequences first longer
- ipv6 multiple zero sequences first longer head
- ipv6 multiple zero sequences second longer
- ipv6 multiple zero sequences second longer tail
- ipv6 multiple zero sequences equal length
- ipv6 all zeros
- ipv6 x all zeros
- ipv6 all zeros x
- ipv6 many zeros but no compression
- traffic direction ipv4 test
- traffic type multicast ipv4 test
- traffic type multicast ipv6 test
- traffic type host local broadcast test
- traffic type host directed broadcast test
- is local connection ipv4 test
- is local connection ipv6 test
- is local connection ipv4 2 test
- is local connection ipv4 multicast test
- is local connection ipv6 multicast test
- is local connection ipv4 link local test
- is local connection ipv6 link local test
- get service simple only one valid
- get service well known ports always win
- get service direction bonus matters
- get service multicast bonus matters
- get service broadcast bonus matters
- get service different tcp udp
- get service not applicable
- get service unknown
- all services map key and values are valid
- service names of old application protocols
- other service names
### src/test/data/diffs/stratified/rust-tauri-apps-tauri-add-doccomment-empty-line/after.rs.test
- defaults
### src/test/data/diffs/stratified/rust-tauri-apps-tauri-add-doccomment-empty-line/before.rs.test
- defaults
### src/test/data/diffs/stratified/tsx-mui-material-ui-add-to-empty-block/after.tsx.test
- <CssBaseline /> › To do
### src/test/fixtures/defects4j/java_defects4j_chart_10_standardtooltiptagfragmentgenerator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_11_shapeutilities.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_12_multiplepieplot.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_13_borderarrangement.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_14_categoryplot.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_14_xyplot.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_15_pieplot.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_16_defaultintervalcategorydataset.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_17_timeseries.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_18_defaultkeyedvalues.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_18_defaultkeyedvalues2d.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_19_categoryplot.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_1_abstractcategoryitemrenderer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_20_valuemarker.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_21_defaultboxandwhiskercategorydataset.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_22_keyedobjects2d.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_23_minmaxcategoryrenderer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_24_graypaintscale.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_25_statisticalbarrenderer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_26_axis.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_2_datasetutilities.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_3_timeseries.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_4_xyplot.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_5_xyseries.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_6_shapelist.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_7_timeperiodvalues.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_8_week.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_chart_9_timeseries.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_10_parser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_11_helpformatter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_12_gnuparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_13_argumentimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_13_writeablecommandline.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_13_writeablecommandlineimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_14_groupimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_15_writeablecommandlineimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_16_groupimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_16_option.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_16_optionimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_16_writeablecommandlineimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_17_posixparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_18_posixparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_19_posixparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_1_commandline.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_20_posixparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_21_groupimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_21_writeablecommandline.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_21_writeablecommandlineimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_22_posixparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_23_helpformatter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_24_helpformatter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_25_helpformatter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_26_optionbuilder.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_27_optiongroup.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_28_parser.rs
- mapping
- invariants
- painting
### src/test/fixtures/defects4j/java_defects4j_cli_29_util.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_2_posixparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_30_defaultparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_31_helpformatter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_31_option.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_31_optionbuilder.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_34_option.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_34_optionbuilder.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_35_options.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_3_typehandler.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_40_typehandler.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_5_util.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_cli_8_helpformatter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_102_normalize.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_103_controlflowanalysis.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_104_uniontype.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_106_jsdocinfobuilder.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_107_commandlinerunner.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_10_nodeutil.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_113_processclosureprimitives.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_114_nameanalyzer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_119_globalnamespace.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_11_typecheck.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_123_codegenerator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_124_exploitassigns.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_125_typecheck.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_130_collapseproperties.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_133_jsdocinfoparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_135_devirtualizeprototypemethods.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_137_normalize.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_138_closurereverseabstractinterpreter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_13_peepholeoptimizationspass.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_144_functiontype.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_147_checkglobalthis.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_149_commandlinerunner.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_14_controlflowanalysis.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_165_recordtypebuilder.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_168_typedscopecreator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_18_compiler.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_19_chainablereverseabstractinterpreter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_1_removeunusedvars.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_28_inlinecostestimator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_30_flowsensitiveinlinevariables.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_31_compiler.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_34_codeprinter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_37_nodetraversal.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_38_codeconsumer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_44_codeconsumer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_52_codegenerator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_57_closurecodingconvention.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_62_lightweightmessageformatter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_65_codegenerator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_66_typecheck.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_67_analyzeprototypeproperties.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_70_typedscopecreator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_71_checkaccesscontrols.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_72_functiontoblockmutator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_72_renamelabels.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_73_codegenerator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_77_codegenerator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_78_peepholefoldconstants.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_79_normalize.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_79_varcheck.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_80_nodeutil.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_86_nodeutil.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_89_globalnamespace.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_90_functiontypebuilder.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_closure_92_processclosureprimitives.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_10_caverphone.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_16_base32.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_17_stringutils.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_1_caverphone.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_1_metaphone.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_1_soundexutils.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_2_base64.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_3_doublemetaphone.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_4_base64.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_7_base64.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_8_base64inputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_codec_9_base64.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_collections_26_multikey.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_11_archivestreamfactory.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_19_zip64extendedinformationextrafield.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_1_cpioarchiveoutputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_23_coders.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_25_ziparchiveinputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_26_ioutils.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_29_cpioarchiveinputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_29_cpioarchiveoutputstream.rs
- mapping
- invariants
- painting
### src/test/fixtures/defects4j/java_defects4j_compress_29_dumparchiveinputstream.rs
- mapping
- invariants
- painting
### src/test/fixtures/defects4j/java_defects4j_compress_29_tararchiveinputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_29_tararchiveoutputstream.rs
- mapping
- invariants
- painting
### src/test/fixtures/defects4j/java_defects4j_compress_29_ziparchiveinputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_33_deflatecompressorinputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_38_tararchiveentry.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_42_unixstat.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_42_ziparchiveentry.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_44_checksumcalculatinginputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_4_changesetperformer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_4_cpioarchiveoutputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_4_tararchiveoutputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_compress_4_ziparchiveoutputstream.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_11_csvparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_12_csvformat.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_13_csvformat.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_14_csvformat.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_1_extendedbufferedreader.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_2_csvrecord.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_4_csvparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_5_csvprinter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_csv_6_csvrecord.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_gson_11_typeadapters.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_gson_13_jsonreader.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_gson_15_jsonwriter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_gson_5_iso8601utils.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_gson_6_jsonadapterannotationtypeadapterfactory.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_11_bytequadscanonicalizer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_12_utf8streamjsonparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_13_jsongeneratorimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_21_filteringparserdelegate.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_25_readerbasedjsonparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_26_nonblockingjsonparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_3_utf8streamjsonparser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_5_jsonpointer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksoncore_8_textbuffer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_103_basicbeandescription.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_103_basicdeserializerfactory.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_103_databindcontext.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_103_deserializercache.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_103_stddeserializer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_103_stdvalueinstantiator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_105_jdkdeserializers.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_107_typedeserializerbase.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_111_atomicreferencedeserializer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_15_beanserializerfactory.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_15_javatype.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_15_stdserializer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_16_annotationmap.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_17_objectmapper.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_1_beanpropertywriter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_20_objectnode.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_25_simpleabstracttyperesolver.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_34_numberserializer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_39_nullifyingdeserializer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_49_writableobjectid.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_59_typefactory.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_79_objectidinfo.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_94_subtypevalidator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jacksondatabind_99_referencetype.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_16_documenttype.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_17_treebuilderstate.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_24_tokeniserstate.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_26_cleaner.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_2_parser.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_31_token.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_31_tokeniserstate.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_33_htmltreebuilder.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_35_htmltreebuilderstate.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_39_datautil.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_40_documenttype.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_52_xmldeclaration.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_54_w3cdom.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_55_tokeniserstate.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_56_documenttype.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_76_htmltreebuilderstate.rs
- mapping
- invariants
- painting
### src/test/fixtures/defects4j/java_defects4j_jsoup_79_leafnode.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_86_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_91_uncheckedioexception.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_92_parsesettings.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_92_xmltreebuilder.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jsoup_93_formelement.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jxpath_15_unioncontext.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jxpath_18_attributecontext.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jxpath_7_coreoperationgreaterthan.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jxpath_7_coreoperationgreaterthanorequal.rs
- mapping
- invariants
- painting
### src/test/fixtures/defects4j/java_defects4j_jxpath_7_coreoperationlessthan.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jxpath_7_coreoperationlessthanorequal.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_jxpath_7_coreoperationrelationalexpression.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_14_stringutils.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_17_charsequencetranslator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_19_numericentityunescaper.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_28_numericentityunescaper.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_38_fastdateformat.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_43_extendedmessageformat.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_4_lookuptranslator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_51_booleanutils.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_54_localeutils.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_64_valuedenum.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_lang_6_charsequencetranslator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_103_normaldistributionimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_10_dscompiler.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_14_weight.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_22_fdistribution.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_22_uniformrealdistribution.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_35_elitisticlistpopulation.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_6_baseoptimizer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_6_cmaesoptimizer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_70_bisectionsolver.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_math_95_fdistributionimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_11_delegatingmethod.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_12_genericmaster.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_15_finalmockcandidatefilter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_17_mocksettingsimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_19_finalmockcandidatefilter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_19_mockcandidatefilter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_19_namebasedcandidatefilter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_19_typebasedcandidatefilter.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_21_constructorinstantiator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_22_equality.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_29_same.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_2_timer.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_30_returnssmartnulls.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_31_returnssmartnulls.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_32_spyannotationengine.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_37_answersvalidator.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_38_argumentmatchingtool.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_5_verificationovertimeimpl.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_7_genericmetadatasupport.rs
- mapping
- painting
- invariants
### src/test/fixtures/defects4j/java_defects4j_mockito_9_callsrealmethods.rs
- mapping
- painting
- invariants
### src/test/fixtures/full/c_awslabs_aws_c_common_only_insert.rs
- mapping
- invariants
### src/test/fixtures/full/c_graph_algorithms_edge_addition_planarity_suite_real_change_all_across_the_file.rs
- mapping
- invariants
### src/test/fixtures/full/c_intel_vpl_gpu_rt_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/full/c_libtom_libtomcrypt_change_function_call.rs
- mapping
- invariants
### src/test/fixtures/full/c_ondsel_development_ondselsolver_add_include.rs
- mapping
- invariants
### src/test/fixtures/full/c_pixaranimationstudios_opensubdiv_change_license_comment.rs
- mapping
- invariants
### src/test/fixtures/full/c_pocoproject_poco_replace_0_with_nullptr.rs
- mapping
- invariants
### src/test/fixtures/full/c_relianoid_nftlb_zcu_log_to_u_log.rs
- mapping
- invariants
### src/test/fixtures/full/c_sched_ext_scx_many_many_moves_some_deletes_some_adds.rs
- mapping
- invariants
### src/test/fixtures/full/c_tripwire_tripwire_open_source_add_single_item_to_list.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_arximboldi_lager_add_two_test_cases.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_bolero_murakami_sprout_copyright_comment_update.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_bolero_murakami_sprout_copyright_update.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_bolero_murakami_sprout_copyright_update_3.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_cgdb_cgdb_struct_to_pointer.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_jpnurmi_znc_playback_adding_new_feature.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_lancos_ponyprog_remove_a_few_lines.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_llnl_sundials_comment_change.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_mikepopoloski_slang_remove_if_condition_and_brackets.rs
- mapping
- invariants
### src/test/fixtures/full/cpp_qpdf_qpdf_move_to_assert_from_if_and_throw.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_cyanfish_naps2_add_condition_to_if.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_dotnet_script_dotnet_script_remove_two_arguments.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_glibsharp_gtksharp_formatting_only_change.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_glibsharp_gtksharp_interesting_case_where_most_should_be_flagged_as_insert_delete_with_a_single_update.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_glibsharp_gtksharp_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_glibsharp_gtksharp_whitespace_only_change_2.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_glibsharp_gtksharp_whitespace_only_change_3.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_icsharpcode_avaloniailspy_a_few_formatting_changes_and_use_a_struct_instead_of_tuples.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_valvesoftware_openvr_add_many_constants_and_fields.rs
- mapping
- invariants
### src/test/fixtures/full/csharp_waf_csharprepl_update_string_url.rs
- mapping
- invariants
### src/test/fixtures/full/css_fortawesome_font_awesome_update_version_comment_2.rs
- mapping
- invariants
### src/test/fixtures/full/css_fortawesome_font_awesome_update_version_comment_3.rs
- mapping
- invariants
### src/test/fixtures/full/css_fortawesome_font_awesome_upgrade_version_comment.rs
- mapping
- invariants
### src/test/fixtures/full/css_heroic_games_launcher_heroicgameslauncher_add_6_lines.rs
- mapping
- invariants
### src/test/fixtures/full/css_horst3180_vertex_theme_add_spaces_to_change_class_selector_into_descendant_selectors.rs
- mapping
- invariants
### src/test/fixtures/full/css_horst3180_vertex_theme_remove_single_rule.rs
- mapping
- invariants
### src/test/fixtures/full/css_lassekongo83_zuki_themes_fails_to_parse.rs
- mapping
- invariants
### src/test/fixtures/full/css_madmaxms_theme_obsidian_2_add_gnome_44_and_a_few_changes.rs
- mapping
- invariants
### src/test/fixtures/full/css_source_foundry_hack_change_font_string.rs
- mapping
- invariants
### src/test/fixtures/full/go_cloudflare_cfssl_change_zero_value_to_is_zero.rs
- mapping
- invariants
### src/test/fixtures/full/go_containers_storage_add_two_constants.rs
- mapping
- invariants
### src/test/fixtures/full/go_cri_o_cri_o_change_importa.rs
- mapping
- invariants
### src/test/fixtures/full/go_darylhjd_mangadesk_remove_a_function_uptade_comments.rs
- mapping
- invariants
### src/test/fixtures/full/go_dweymouth_supersonic_remove_two_lines.rs
- mapping
- invariants
### src/test/fixtures/full/go_henri_gasc_cliphist_auto_generated_file.rs
- mapping
- invariants
### src/test/fixtures/full/go_jesseduffield_lazygit_add_function.rs
- mapping
- invariants
### src/test/fixtures/full/go_nwg_piotr_gopsuinfo_shuffle_around_if_blocks.rs
- mapping
- invariants
### src/test/fixtures/full/go_prometheus_node_exporter_remove_one_comment.rs
- mapping
- invariants
### src/test/fixtures/full/go_stackexchange_blackbox_add_comment.rs
- mapping
- invariants
### src/test/fixtures/full/html_abs_lang_abs_version_update_in_string.rs
- mapping
- invariants
### src/test/fixtures/full/html_berndporr_iir1_a_lot_of_new_functionality.rs
- mapping
- invariants
### src/test/fixtures/full/html_chennes_med_extreme_test.rs
- mapping
- invariants
### src/test/fixtures/full/html_chennes_med_only_text_value_change.rs
- mapping
- invariants
### src/test/fixtures/full/html_chennes_med_only_text_value_change_2.rs
- mapping
- invariants
### src/test/fixtures/full/html_milkytracker_milkytracker_text_update.rs
- mapping
- invariants
### src/test/fixtures/full/html_oauth_xx_oauth_ruby_version_and_timestamp_update.rs
- mapping
- invariants
### src/test/fixtures/full/html_tcltk_thread_one_multiline_value_changed.rs
- mapping
- invariants
### src/test/fixtures/full/html_xiaoyeli_superlu_change_a_bunch_of_values_and_add_one_element.rs
- mapping
- invariants
### src/test/fixtures/full/java_eclipse_jdt_eclipse_remove_import_and_inheritance.rs
- mapping
- invariants
### src/test/fixtures/full/java_hunterhacker_jdom_javadoc_update.rs
- mapping
- invariants
### src/test/fixtures/full/java_hunterhacker_jdom_move_a_block.rs
- mapping
- invariants
### src/test/fixtures/full/java_jakartaee_rest_real_logic_change_of_a_significant_chunck.rs
- mapping
- invariants
### src/test/fixtures/full/java_jflex_de_jflex_javadoc_update.rs
- mapping
- invariants
### src/test/fixtures/full/java_jopt_simple_jopt_simple_remove_import.rs
- mapping
- invariants
### src/test/fixtures/full/java_junit_pioneer_junit_pioneer_add_block_comment.rs
- mapping
- invariants
### src/test/fixtures/full/java_pdftk_java_pdftk_license_comment_change.rs
- mapping
- invariants
### src/test/fixtures/full/java_pdftk_java_pdftk_real_change_all_across_the_file.rs
- mapping
- invariants
### src/test/fixtures/full/java_zeroc_ice_ice_comment_only_update.rs
- mapping
- invariants
### src/test/fixtures/full/javascript_grobian_carbonapi_web_add_binary_expression_to_existing_assignment.rs
- mapping
- invariants
### src/test/fixtures/full/javascript_home_sweet_gnome_dash_to_panel_change_set_style_arguments.rs
- mapping
- invariants
### src/test/fixtures/full/javascript_jquery_ui_rails_jquery_ui_rails_add_strict_and_move_function_call_parenthesis.rs
- mapping
- invariants
### src/test/fixtures/full/javascript_jquery_ui_rails_jquery_ui_rails_update_text_in_string.rs
- mapping
- invariants
### src/test/fixtures/full/javascript_jquery_ui_rails_jquery_ui_rails_update_text_in_string_2.rs
- mapping
- invariants
### src/test/fixtures/full/javascript_usebruno_bruno_add_class_to_classname_attribute.rs
- mapping
- invariants
### src/test/fixtures/full/javascript_usebruno_bruno_style_change.rs
- mapping
- invariants
### src/test/fixtures/full/javascript_xpra_org_xpra_html5_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/json_fedoraqt_qgnomeplatform_remove_from_list.rs
- mapping
- invariants
### src/test/fixtures/full/json_fortawesome_font_awesome_string_change.rs
- mapping
- invariants
### src/test/fixtures/full/json_governikus_ausweisapp_whitespace_only.rs
- mapping
- invariants
### src/test/fixtures/full/json_ipfs_ipfs_desktop_only_update_version_strings.rs
- mapping
- invariants
### src/test/fixtures/full/json_iwalton3_jellyfin_web_jmp_gigantic_file_trivial_string_version_change.rs
- mapping
- invariants
### src/test/fixtures/full/json_kellyjonbrazil_jc_add_maxhops_to_traceroute.rs
- mapping
- invariants
### src/test/fixtures/full/json_kiwix_kiwix_desktop_add_a_few_change_a_few.rs
- mapping
- invariants
### src/test/fixtures/full/json_main_branch_track_open_instances_remove_item_from_list.rs
- mapping
- invariants
### src/test/fixtures/full/json_webgpu_native_webgpu_headers_only_insertions.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_jetbrains_kotlin_add_single_const.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_jetbrains_kotlin_remove_one_comment_line.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_jetbrains_kotlin_remove_some_deprecation.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_jetbrains_kotlin_remove_typealias_with_annotations.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_yairm210_unciv_another_vector2_removal.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_yairm210_unciv_move_from_vec2_to_hexexpression.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_yairm210_unciv_remove_tovector2_from_multiple_callsites.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_yairm210_unciv_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_yairm210_unciv_vector2_to_hexgrid.rs
- mapping
- invariants
### src/test/fixtures/full/kotlin_yairm210_unciv_yet_another_vector2_removal.rs
- mapping
- invariants
### src/test/fixtures/full/lua_corsixth_corsixth_refactor_if_expressions.rs
- mapping
- invariants
### src/test/fixtures/full/lua_dromozoa_dromozoa_utf8_only_comment_text_update.rs
- mapping
- invariants
### src/test/fixtures/full/lua_luakit_luakit_actual_test_change_merging_two_tests_into_one.rs
- mapping
- invariants
### src/test/fixtures/full/lua_luals_lua_language_server_add_gsub_function_call.rs
- mapping
- invariants
### src/test/fixtures/full/lua_luals_lua_language_server_add_two_translations.rs
- mapping
- invariants
### src/test/fixtures/full/lua_luaposix_luaposix_copyright_year_change.rs
- mapping
- invariants
### src/test/fixtures/full/lua_return_to_the_roots_s25client_add_two_items_to_list.rs
- mapping
- invariants
### src/test/fixtures/full/lua_return_to_the_roots_s25client_few_added_lines.rs
- mapping
- invariants
### src/test/fixtures/full/lua_teeworlds_teeworlds_add_or_expression_to_existing_if.rs
- mapping
- invariants
### src/test/fixtures/full/php_consol_monitoring_pnp_move_from_one_function_to_other_that_has_more_params.rs
- mapping
- invariants
### src/test/fixtures/full/php_doctrine_orm_delete_3_comments_and_update_1.rs
- mapping
- invariants
### src/test/fixtures/full/php_icinga_icingaweb2_module_graphite_add_a_few_nodes.rs
- mapping
- invariants
### src/test/fixtures/full/php_php_fig_log_deleted_a_class.rs
- mapping
- invariants
### src/test/fixtures/full/php_rk4an_phpsysinfo_actual_logic_change.rs
- mapping
- invariants
### src/test/fixtures/full/php_smarty_php_smarty_change_version_string.rs
- mapping
- invariants
### src/test/fixtures/full/php_symfony_finder_move_elseif_block_to_else.rs
- mapping
- invariants
### src/test/fixtures/full/php_theseer_directoryscanner_add_two_test_cases_and_reformat_file.rs
- mapping
- invariants
### src/test/fixtures/full/php_zetacomponents_base_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/full/php_zetacomponents_consoletools_file_with_parse_errors_and_a_few_deletions.rs
- mapping
- invariants
### src/test/fixtures/full/python_aajanki_yle_dl_tiny_change.rs
- mapping
- invariants
### src/test/fixtures/full/python_aboutcode_org_license_expression_excellent_test_case.rs
- mapping
- invariants
### src/test/fixtures/full/python_bolero_murakami_sprout_change_copyright_year.rs
- mapping
- invariants
### src/test/fixtures/full/python_cloudflare_cloudflare_python_comment_only_changes.rs
- mapping
- invariants
### src/test/fixtures/full/python_espressomd_espresso_fix_comment_typo.rs
- mapping
- invariants
### src/test/fixtures/full/python_fuzzyray_esearch_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/full/python_open_telemetry_opentelemetry_python_move_from_one_function_to_other.rs
- mapping
- invariants
### src/test/fixtures/full/python_persepolisdm_persepolis_add_a_single_node.rs
- mapping
- invariants
### src/test/fixtures/full/python_portagefilelist_client_remove_one_import_and_update_one_const_string.rs
- mapping
- invariants
### src/test/fixtures/full/python_zaneb_autopage_move_time_sleep.rs
- mapping
- invariants
### src/test/fixtures/full/r_gtownsend_icon_one_letter_identifier_change.rs
- mapping
- invariants
### src/test/fixtures/full/r_hroptatyr_dateutils_real_changes_to_an_r_script.rs
- mapping
- invariants
### src/test/fixtures/full/r_mtytel_helm_dummy_file.rs
- mapping
- invariants
### src/test/fixtures/full/r_oracle_dtrace_utils_i_have_no_idea_what_this_change_actually_does.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_intridea_multi_json_remove_double_colon.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_intridea_multi_json_remove_single_comma.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_jmespath_jmespath_formatting_and_style_guide_fixes.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_jmespath_jmespath_go_from_conditional_to_unless.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_main_branch_process_executer_actual_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_marcandre_backports_actual_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_mikel_mail_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_moneta_rb_moneta_change_one_identifier.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_moneta_rb_moneta_trivial_version_update.rs
- mapping
- invariants
### src/test/fixtures/full/ruby_rapid7_ruby_smb_add_import_and_one_expression.rs
- mapping
- invariants
### src/test/fixtures/full/rust_fornwall_rust_script_add_lifecycle_management.rs
- mapping
- invariants
### src/test/fixtures/full/rust_quietvoid_dovi_tool_change_to_templated_call.rs
- mapping
- invariants
### src/test/fixtures/full/rust_rbspy_rbspy_add_two_test_cases.rs
- mapping
- invariants
### src/test/fixtures/full/rust_refirmlabs_binwalk_whitespace_only.rs
- mapping
- invariants
### src/test/fixtures/full/rust_skim_rs_skim_format_string.rs
- mapping
- invariants
### src/test/fixtures/full/rust_tiffany352_rink_rs_real_change.rs
- mapping
- invariants
### src/test/fixtures/full/rust_tursodatabase_turso_unwrap_to_expect.rs
- mapping
- invariants
### src/test/fixtures/full/rust_weggli_rs_weggli_move_import_around_and_formatting_change.rs
- mapping
- invariants
### src/test/fixtures/full/rust_xou816_spot_add_some_around_existing_code.rs
- mapping
- invariants
### src/test/fixtures/full/rust_yannjor_krabby_actual_normal_change.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_add_a_function_call.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_add_a_member_to_expression_chains.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_add_one_argument.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_assert_format_string_change.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_change_from_one_function_to_other_and_delete_a_function.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_expand_import_path.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_new_import_used_to_remove_a_lot_of_code.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_only_change_failure_strings.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_real_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_small_refactoring.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_split_import.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_split_import_2.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_split_two_asserts_into_six_two_times.rs
- mapping
- invariants
### src/test/fixtures/full/scala_com_lihaoyi_mill_version_string_change.rs
- mapping
- invariants
### src/test/fixtures/full/scala_sirthias_parboiled_value_change.rs
- mapping
- invariants
### src/test/fixtures/full/scala_xerial_xerial_java_actual_code_change.rs
- mapping
- invariants
### src/test/fixtures/full/scala_ymnk_jzlib_add_a_test_case.rs
- mapping
- invariants
### src/test/fixtures/full/scala_ymnk_jzlib_add_one_test_case.rs
- mapping
- invariants
### src/test/fixtures/full/scala_ymnk_jzlib_interesting_probably_has_no_optimal_solution.rs
- mapping
- painting
- invariants
### src/test/fixtures/full/shellscript_docker_docker_bench_security_move_all_functions_by_one_and_add_one_to_the_end.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_fleetingheart_ksre_multiline_string_change.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_go_delve_delve_add_if.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_hgst_libzbc_add_variable.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_imapsync_imapsync_add_some_test_cases_and_a_few_other_changes.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_maxsatula_ocp_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_nicolargo_glances_add_shebang.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_nomad_software_vend_huge_multi_line_strings_updated.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_openlightingproject_ola_real_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/shellscript_ropery_ffcast_comment_only_change.rs
- mapping
- invariants
### src/test/fixtures/full/swift_apple_swift_argument_parser_if_to_guard.rs
- mapping
- invariants
### src/test/fixtures/full/swift_apple_swift_argument_parser_refactor_and_improve_tests.rs
- mapping
- invariants
### src/test/fixtures/full/swift_apple_swift_argument_parser_remove_trimming_lines_function_call.rs
- mapping
- invariants
### src/test/fixtures/full/swift_apple_swift_argument_parser_simplify_code.rs
- mapping
- invariants
### src/test/fixtures/full/swift_apple_swift_argument_parser_small_change.rs
- mapping
- invariants
### src/test/fixtures/full/swift_logseq_logseq_add_real_feature_keypress_tracking.rs
- mapping
- invariants
### src/test/fixtures/full/swift_logseq_logseq_insertions_only.rs
- mapping
- invariants
### src/test/fixtures/full/swift_swift_emacs_swift_mode_seems_like_test_code.rs
- mapping
- invariants
### src/test/fixtures/full/swift_tree_sitter_tree_sitter_haskell_remove_a_list_item.rs
- mapping
- invariants
### src/test/fixtures/full/swift_zeroc_ice_ice_add_an_if_block_and_variable.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_greenbone_gsa_add_import_and_use_it.rs
- mapping
- invariants
- painting
### src/test/fixtures/full/tsx_keybase_client_change_from_one_import_and_call_to_another.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_keybase_client_emoji_to_native.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_kong_insomnia_classname_strings_changed.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_kong_insomnia_if_to_ternary_operator.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_kong_insomnia_rewrite_if_using_ternary_twice.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_mitmproxy_mitmproxy_array_to_object.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_popcorn_official_popcorn_desktop_simple_property_rename.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_rektdeckard_departure_mono_import_path.rs
- mapping
- invariants
### src/test/fixtures/full/tsx_troyeguo_koodo_reader_pure_insert.rs
- mapping
- invariants
### src/test/fixtures/full/typescript_apache_echarts_envelop_2_lines_with_an_if_block.rs
- mapping
- invariants
### src/test/fixtures/full/typescript_lxqt_lxqt_openssh_askpass_change_url.rs
- mapping
- invariants
### src/test/fixtures/full/typescript_lxqt_lxqt_panel_not_actually_ts_but_still.rs
- mapping
- invariants
### src/test/fixtures/full/typescript_th_ch_youtube_music_add_a_function_and_some_small_changes.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_chikamichi_mediawiki_add_one_autocmd.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_chikamichi_mediawiki_remove_3_lines.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_fedorenchik_qt_support_add_two_lines.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_fholgado_minibufexpl_massive_comment_reduction.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_idanarye_vim_merginal_add_one_call.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_jreybert_vimagit_move_one_line.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_m_pilia_vim_mediawiki_add_a_new_functon.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_preservim_tagbar_add_if_statements.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_protesilaos_tempus_themes_vim_add_terminal_color_scheme.rs
- mapping
- invariants
### src/test/fixtures/full/vimscript_protesilaos_tempus_themes_vim_change_two_values.rs
- mapping
- invariants
### src/test/fixtures/full/xml_adoptopenjdk_icedtea_web_update_version_string.rs
- mapping
- invariants
### src/test/fixtures/full/xml_antlr_antlr3_comment_out_part_of_code_interesting_case.rs
- mapping
- invariants
### src/test/fixtures/full/xml_eclipse_ee4j_jaxb_istack_commons_update_version_string.rs
- mapping
- invariants
### src/test/fixtures/full/xml_fasterxml_jackson_dataformat_xml_only_string_value_change.rs
- mapping
- invariants
### src/test/fixtures/full/xml_gap_packages_toric_remove_two_attributes.rs
- mapping
- invariants
### src/test/fixtures/full/xml_gpuopen_drivers_amdvlk_version_update.rs
- mapping
- invariants
### src/test/fixtures/full/xml_javacc_javacc_version_update.rs
- mapping
- invariants
### src/test/fixtures/full/xml_lincity_ng_lincity_ng_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/full/xml_megaglest_megaglest_data_update_url_path.rs
- mapping
- invariants
### src/test/fixtures/full/xml_veracrypt_veracrypt_version_string.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_arkq_bluez_alsa_version_string.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_containers_skopeo_remove_one_block_pair.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_draios_sysdig_const_string_url_change.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_draios_sysdig_string_url_change.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_draios_sysdig_string_url_change_2.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_draios_sysdig_string_url_change_3.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_draios_sysdig_string_url_change_4.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_kbudde_rabbitmq_exporter_string_url_change.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_manugarg_pacparser_string_url_change.rs
- mapping
- invariants
### src/test/fixtures/full/yaml_python_xmp_toolkit_python_xmp_toolkit_insert_two_values_to_a_slice.rs
- mapping
- invariants
### src/test/fixtures/handmade/bazel_not_actually_supported_by_treesitter.rs
- painting
- invariants
### src/test/fixtures/handmade/cpp_add_const_correctness.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/cpp_add_memory_management.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/cpp_add_templates.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/cpp_fix_segfault.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/cpp_optimize_algorithm.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/java_add_exception_handling.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/java_add_interface.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/java_add_logging.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/java_fix_array_index.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/java_refactor_constants.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/javascript_add_array_method.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/javascript_add_destructuring.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/javascript_add_event_listener.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/javascript_fix_promises.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/javascript_refactor_arrow_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/kotlin_add_data_class.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/kotlin_add_null_check.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/kotlin_add_validation.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/kotlin_fix_loop_bug.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/kotlin_refactor_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/python_add_remove_block.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/python_added_if_block.rs
- mapping details
- mapping
- painting
- invariants
### src/test/fixtures/handmade/python_added_if_block_small.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/python_api_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/python_bugfix_loop.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/python_refactoring.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_add_comments_and_real_new_logic.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_add_if.rs
- mapping details
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_add_to_existing_use.rs
- mapping details
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_add_value_to_enum.rs
- mapping details
- mapping
- mapping details reversed
- painting
- invariants
### src/test/fixtures/handmade/rust_adding_a_variable_and_test_with_comments.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_adding_many_identical_cfg_test_statements_to_a_signle_file_doesnt_prefer_the_local_insert_but_rather_goes_to_some_other_existing_cfg.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_adding_to_a_list_of_identical_attributes_should_favour_near_matches.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_algorithm_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_cost_optimization.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_data_structure.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_error_handling.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_firefox_webrenderer_borders.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_hash_optimization.rs
- mapping details
- painting
- invariants
### src/test/fixtures/handmade/rust_hello_world_added_message.rs
- mapping details
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_hello_world_removed_message.rs
- mapping details
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_leetcode_1_bugfix.rs
- mapping details
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_multi_map_duplicate_calls.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_next_font_imports_generator.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_no_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_real_logic_change_in_a_huge_75k_node_file.rs
- mapping
- invariants
### src/test/fixtures/handmade/rust_small_addition_with_reuse_of_binary_expressions.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_sniffnet_protocol.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_tauri_api_build_1.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_tauri_api_build_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_tauri_cli_ios_dev.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_turbopack_module_rule.rs
- mapping
- mapping details
- invariants
- painting
### src/test/fixtures/handmade/rust_turbopack_persistence_tools_main.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_zed_git_panel_settings.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/rust_zed_workspace_tasks.rs
- mapping
- invariants
### src/test/fixtures/handmade/typescript_add_error_handling.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/typescript_add_generics.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/typescript_add_type_annotations.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/typescript_async_await.rs
- mapping
- painting
- invariants
### src/test/fixtures/handmade/typescript_refactor_interface.rs
- mapping
- painting
- invariants
### src/test/fixtures/small/c_cpython_autogenerated_code.rs
- mapping
- painting
- invariants
### src/test/fixtures/small/c_ffmpeg_added_typedef_to_enum.rs
- mapping
- painting
- invariants
### src/test/fixtures/small/c_freeciv_add_parameter_to_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/small/c_htop_remove_function_declaration.rs
- mapping
- painting
- invariants
### src/test/fixtures/small/c_linux_small_bugfix.rs
- mapping
- invariants
- painting
### src/test/fixtures/small/c_linux_small_change.rs
- mapping
- invariants
### src/test/fixtures/small/c_linux_small_change_struct_to_char.rs
- mapping
- invariants
### src/test/fixtures/small/c_microsoft_terminal_add_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/small/c_nginx_add_typedef.rs
- mapping
- invariants
### src/test/fixtures/small/c_postgres_real_logic_change.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_godot_small_bugfix.rs
- mapping
- invariants
- painting
### src/test/fixtures/small/cpp_ladybird_refactor_variables_if_changes.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_laydbird_change_function_signature.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_libreoffice_add_const.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_libreoffice_remove_return_value.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_nextcloud_add_test_case.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_ollama_add_function_argument.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_opencv_add_test_case.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_tensorflow_switch_to_primitive_types.rs
- mapping
- invariants
### src/test/fixtures/small/cpp_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_jellyfin_add_function.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_jellyfin_sql_query_fix.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_lidarr_add_function.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_lidarr_call_different_function.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_lidarr_condition_change.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_lidarr_new_feature.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_radarr_add_object_instance.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_sonarr_add_if_block.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_sonarr_add_true_to_function_call.rs
- mapping
- invariants
### src/test/fixtures/small/csharp_sonarr_change_type.rs
- mapping
- invariants
### src/test/fixtures/small/css_add_property.rs
- mapping
- invariants
### src/test/fixtures/small/css_mozilla_firefox_firefox_actual_style_changes.rs
- mapping
- invariants
### src/test/fixtures/small/css_playwright_add_class_selector.rs
- mapping
- invariants
### src/test/fixtures/small/css_shadcn_ui_ui_completely_broken_treesitter_parsing.rs
- mapping
- invariants
### src/test/fixtures/small/css_wordpress_add_comment.rs
- mapping
- invariants
### src/test/fixtures/small/css_wordpress_format_comment_and_fix.rs
- mapping
- invariants
### src/test/fixtures/small/css_wordpress_reformat.rs
- mapping
- invariants
### src/test/fixtures/small/css_wordpress_smalll_change.rs
- mapping
- invariants
### src/test/fixtures/small/css_wordpress_wordpress_autogenerated_file.rs
- mapping
- invariants
### src/test/fixtures/small/css_wordpress_wordpress_change_simple_values_to_vars.rs
- mapping
- invariants
### src/test/fixtures/small/go_caddy_rename_type.rs
- mapping
- invariants
### src/test/fixtures/small/go_gin_add_function.rs
- mapping
- invariants
### src/test/fixtures/small/go_gin_change_import.rs
- mapping
- invariants
### src/test/fixtures/small/go_gin_constant_change.rs
- mapping
- invariants
### src/test/fixtures/small/go_kubernetes_kubernetes_add_unit_test_cases.rs
- mapping
- invariants
### src/test/fixtures/small/go_lazygit_add_to_if_condition.rs
- mapping
- invariants
### src/test/fixtures/small/go_lazygit_switch_to_strings.rs
- mapping
- painting
- invariants
### src/test/fixtures/small/go_ollama_ollama_surround_block_with_if_and_function_call.rs
- mapping
- invariants
### src/test/fixtures/small/go_prometheus_single_comment_change.rs
- mapping
- invariants
### src/test/fixtures/small/go_user_slices_library.rs
- mapping
- invariants
### src/test/fixtures/small/html_apache_echarts_actual_structure_change.rs
- mapping
- invariants
### src/test/fixtures/small/html_fatedier_add_attribute.rs
- mapping
- invariants
### src/test/fixtures/small/html_firefox_update_src.rs
- mapping
- invariants
### src/test/fixtures/small/html_gohugoio_hugo_enclose_table_with_div_and_add_thead_tbody.rs
- mapping
- invariants
### src/test/fixtures/small/html_gorhill_add_tag.rs
- mapping
- invariants
### src/test/fixtures/small/html_hugo_tag_to_selfclosing_tag.rs
- mapping
- invariants
### src/test/fixtures/small/html_ladybird_delete_attribute.rs
- mapping
- invariants
### src/test/fixtures/small/html_mermaid_update_link.rs
- mapping
- invariants
### src/test/fixtures/small/html_mozilla_firefox_firefox_remove_li_around_button.rs
- mapping
- invariants
### src/test/fixtures/small/html_twbs_bootstrap_add_the_same_attribute_in_many_places.rs
- mapping
- invariants
### src/test/fixtures/small/java_genymobile_scrcpy_change_some_android_version_constant.rs
- mapping
- invariants
### src/test/fixtures/small/java_genymobile_scrcpy_refactor_for_loop_in_a_function.rs
- mapping
- invariants
### src/test/fixtures/small/java_genymobile_scrcpy_switch_from_three_exceptions_to_one.rs
- mapping
- invariants
### src/test/fixtures/small/java_nextcloud_android_add_if_branch_with_reused_return.rs
- mapping
- invariants
### src/test/fixtures/small/java_nextcloud_android_add_two_function_calls.rs
- mapping
- invariants
### src/test/fixtures/small/java_protobuf_add_two_annotations.rs
- mapping
- invariants
### src/test/fixtures/small/java_protocolbuffers_protobuf_add_a_method.rs
- mapping
- invariants
### src/test/fixtures/small/java_protocolbuffers_protobuf_add_import_and_update_field_access.rs
- mapping
- invariants
### src/test/fixtures/small/java_scrcpy_public_to_protected.rs
- mapping
- invariants
### src/test/fixtures/small/java_scrcpy_remove_or_expression.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_microsoft_typescript_another_test_case_marked_with_strict.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_microsoft_typescript_broken_js_remove_string_fragment.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_microsoft_typescript_test_data_pretending_to_be_code_maybe_broken_parsing.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_mozilla_firefox_add_comment.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_twbs_bootstrap_comment_version_update.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_typescript_add_strict_3.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_typescript_add_use_strict.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_typescript_interesting_small_edit_refactor.rs
- mapping
- painting
- invariants
### src/test/fixtures/small/javascript_typescript_use_strict_2.rs
- mapping
- invariants
### src/test/fixtures/small/javascript_typescript_very_interesting_brokn_code.rs
- mapping
- invariants
### src/test/fixtures/small/json_apache_string_change_version.rs
- mapping
- invariants
### src/test/fixtures/small/json_excalidraw_excalidraw_change_translations_mostly_add.rs
- mapping
- invariants
### src/test/fixtures/small/json_gorhill_ublock_add_5_pairs.rs
- mapping
- invariants
### src/test/fixtures/small/json_langflow_update_single_string.rs
- mapping
- invariants
### src/test/fixtures/small/json_mastodon_add_translation.rs
- mapping
- invariants
### src/test/fixtures/small/json_nextcloud_server_deleted_pair.rs
- mapping
- invariants
### src/test/fixtures/small/json_nextcloud_server_remove_many_and_move_one.rs
- mapping
- invariants
### src/test/fixtures/small/json_radarr_radarr_rename_string_key.rs
- mapping
- invariants
### src/test/fixtures/small/json_shadcn_ui_ui_react_code_in_string_constant.rs
- mapping
- invariants
### src/test/fixtures/small/json_shadcn_ui_ui_string_value_update_string_is_code.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_nextcloud_a_few_small_removals.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_nextcloud_android_add_a_feature.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_nextcloud_android_dot_to_question_mark_dot.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_nextcloud_android_extract_argument_into_variable.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_nextcloud_android_move_from_one_mocking_library_to_other.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_nextcloud_change_function_fingerprint.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_nextcloud_remove_function.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_nextcloud_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_remove_function.rs
- mapping
- invariants
### src/test/fixtures/small/kotlin_small_api_change.rs
- mapping
- invariants
### src/test/fixtures/small/lua_awesome_only_comment_change.rs
- mapping
- invariants
### src/test/fixtures/small/lua_awesomewm_awesome_align_to_halign.rs
- mapping
- invariants
### src/test/fixtures/small/lua_awesomewm_awesome_change_doccomments.rs
- mapping
- invariants
### src/test/fixtures/small/lua_awesomewm_awesome_comment_changes_and_additions.rs
- mapping
- invariants
### src/test/fixtures/small/lua_neovim_neovim_add_if_around_one_line.rs
- mapping
- invariants
### src/test/fixtures/small/lua_neovim_neovim_add_new_logic.rs
- mapping
- invariants
### src/test/fixtures/small/lua_neovim_neovim_constant_changes.rs
- mapping
- invariants
### src/test/fixtures/small/lua_neovim_neovim_if_flips_two_branches.rs
- mapping
- invariants
### src/test/fixtures/small/lua_neovim_neovim_logic_change_with_some_code_re_use.rs
- mapping
- invariants
### src/test/fixtures/small/lua_neovim_one_added_line.rs
- mapping
- invariants
### src/test/fixtures/small/php_nextcloud_change_doccomment.rs
- mapping
- invariants
### src/test/fixtures/small/php_nextcloud_server_whitespace_and_added_declaration.rs
- mapping
- invariants
### src/test/fixtures/small/php_wordpress_wordpress_add_null_to_return.rs
- mapping
- invariants
### src/test/fixtures/small/php_wordpress_wordpress_add_rtc_check.rs
- mapping
- invariants
### src/test/fixtures/small/php_wordpress_wordpress_add_some_checks.rs
- mapping
- invariants
### src/test/fixtures/small/php_wordpress_wordpress_update_doccomment.rs
- mapping
- invariants
### src/test/fixtures/small/php_wordpress_wordpress_version_update.rs
- mapping
- invariants
### src/test/fixtures/small/php_wordpress_wordpress_version_update_2.rs
- mapping
- invariants
### src/test/fixtures/small/php_wordpress_wordpress_version_update_3.rs
- mapping
- invariants
### src/test/fixtures/small/php_wordpress_wordpress_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/small/python_ansible_ansible_field_rename.rs
- mapping
- invariants
### src/test/fixtures/small/python_ansible_ansible_ridiculously_long_yaml_in_string_constant_and_actual_code_changes.rs
- mapping
- invariants
### src/test/fixtures/small/python_django_django_update_unit_tests_actual_logic_change.rs
- mapping
- invariants
### src/test/fixtures/small/python_langflow_ai_langflow_actual_change_of_logic.rs
- mapping
- invariants
### src/test/fixtures/small/python_openhands_openhands_change_string_constant.rs
- mapping
- invariants
### src/test/fixtures/small/python_paddlepaddle_paddleocr_formatting_only_change.rs
- mapping
- invariants
### src/test/fixtures/small/python_pandas_dev_pandas_doccoment_changes_only.rs
- mapping
- invariants
### src/test/fixtures/small/python_pytorch_pytorch_add_param_to_many_places_and_update_one.rs
- mapping
- invariants
### src/test/fixtures/small/python_thefuck_multiline_string_change.rs
- mapping
- invariants
### src/test/fixtures/small/python_ytdl_add_import_and_function_call.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_homebrew_add_or_expression.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_homebrew_brew_actual_logic_change.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_homebrew_brew_change_heredoc_to_string.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_homebrew_brew_decent_amount_of_new_code_and_some_changes.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_homebrew_brew_enclose_block_in_rescue.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_homebrew_brew_function_callsite_argument_change.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_homebrew_brew_real_small_change.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_junegunn_fzf_add_test_case.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_junegunn_fzf_add_test_case_2.rs
- mapping
- invariants
### src/test/fixtures/small/ruby_mastodon_mastodon_use_context_and_new_test_case.rs
- mapping
- invariants
### src/test/fixtures/small/rust_rust_lang_rust_add_key_to_function_arguments_and_call_sites.rs
- mapping
- invariants
### src/test/fixtures/small/rust_rustdesk_add_item.rs
- mapping
- invariants
### src/test/fixtures/small/rust_rustdesk_add_one_slice_element.rs
- mapping
- invariants
### src/test/fixtures/small/rust_rustdesk_rustdesk_actual_logic_change_in_io_loop_medium_sized_file.rs
- mapping
- invariants
### src/test/fixtures/small/rust_rustdesk_rustdesk_add_two_values_to_slice.rs
- mapping
- invariants
### src/test/fixtures/small/rust_rustdesk_rustdesk_large_file_40k_normal_feature_work.rs
- mapping
- invariants
### src/test/fixtures/small/rust_rustdesk_rustdesk_string_constant_change.rs
- mapping
- invariants
### src/test/fixtures/small/rust_vercel_nextjs_move_from_one_struct_to_another_and_add_some_code.rs
- mapping
- invariants
### src/test/fixtures/small/rust_vercel_nextjs_refactoring_would_require_mulitmap_mapping.rs
- mapping
- invariants
### src/test/fixtures/small/rust_zed_industries_zed_add_argument.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_ansible_ansible_add_variable_and_string_expansion.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_ansible_ansible_simple_deletion.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_genymobile_scrcpy_add_two_flags.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_langchain_ai_langchain_some_interesting_raw_string_to_string_content.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_nextcloud_server_change_invocation_string.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_nvm_sh_nvm_upgrade_version_string.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_pi_hole_pi_hole_sql_code_in_string_constant.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_pytorch_pytorch_change_invocation_string.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_scikit_learn_scikit_learn_string_to_regex.rs
- mapping
- invariants
### src/test/fixtures/small/shellscript_torvalds_linux_double_equals_to_equals.rs
- mapping
- invariants
### src/test/fixtures/small/swift_nextcloud_ios_actual_control_flow_change.rs
- mapping
- invariants
### src/test/fixtures/small/swift_nextcloud_ios_actual_logic_added.rs
- mapping
- invariants
### src/test/fixtures/small/swift_nextcloud_ios_add_4_functions.rs
- mapping
- invariants
### src/test/fixtures/small/swift_nextcloud_ios_call_different_function.rs
- mapping
- invariants
### src/test/fixtures/small/swift_nextcloud_ios_move_function_and_refactor_logic.rs
- mapping
- invariants
### src/test/fixtures/small/swift_nextcloud_ios_refactor_and_change.rs
- mapping
- invariants
### src/test/fixtures/small/swift_swiftlang_swift_actual_logic_change.rs
- mapping
- invariants
### src/test/fixtures/small/swift_swiftlang_swift_comment_change.rs
- mapping
- invariants
### src/test/fixtures/small/swift_swiftlang_swift_comment_change_2.rs
- mapping
- invariants
### src/test/fixtures/small/swift_swiftlang_swift_enable_checks_remove_todo_comment.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_apache_superset_add_test_case.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_apache_superset_error_handling_change.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_excalidraw_excalidraw_huge_file_with_real_logic_change.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_excalidraw_excalidraw_import_path_change.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_excalidraw_excalidraw_move_from_one_struct_to_other.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_langflow_ai_langflow_move_function_around_update_string_constant.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_material_remove_import.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_mui_material_ui_move_colour_to_a_new_attribute.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_shadcn_ui_ui_add_attribute.rs
- mapping
- invariants
### src/test/fixtures/small/tsx_shadcn_ui_ui_real_small_change.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_excalidraw_excalidraw_add_function.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_excalidraw_excalidraw_add_values_to_lists.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_microsoft_playwright_add_two_test_cases.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_microsoft_typescript_add_dot_js_to_import_paths.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_microsoft_typescript_add_target_comment.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_microsoft_typescript_comment_change.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_n8n_io_n8n_remove_and_add_imports.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_typescript_add_target_comment.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_typescript_add_target_comment_2.rs
- mapping
- invariants
### src/test/fixtures/small/typescript_typescript_add_target_comment_3.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_junegunn_fzf_condition_canges.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_add_a_few_lines.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_add_a_few_lines_one_after_the_other.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_add_line_comment.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_add_test_case_plus_edit_existing_one.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_add_two_functions_and_modify_a_few_lines.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_awful_test_case_bunch_of_hex_colours_more_data_than_code.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_i_have_no_idea_what_this_diff_does.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_improved_asserts.rs
- mapping
- invariants
### src/test/fixtures/small/vimscript_neovim_neovim_test_debian_package_parsing_awful_string_matching.rs
- mapping
- invariants
### src/test/fixtures/small/xml_microsoft_terminal_multiline_string_constant_change.rs
- mapping
- invariants
### src/test/fixtures/small/xml_mozilla_firefox_firefox_add_a_few_attributes.rs
- mapping
- invariants
### src/test/fixtures/small/xml_mozilla_firefox_firefox_add_a_few_translations_and_a_few_attributes.rs
- mapping
- invariants
### src/test/fixtures/small/xml_nextcloud_android_add_few_translations.rs
- mapping
- invariants
### src/test/fixtures/small/xml_nextcloud_android_delete_element.rs
- mapping
- invariants
### src/test/fixtures/small/xml_nextcloud_android_delete_element_2.rs
- mapping
- invariants
### src/test/fixtures/small/xml_odoo_odoo_add_attribute.rs
- mapping
- invariants
### src/test/fixtures/small/xml_odoo_odoo_add_button_roles.rs
- mapping
- invariants
### src/test/fixtures/small/xml_odoo_odoo_add_two_attributes.rs
- mapping
- invariants
### src/test/fixtures/small/xml_odoo_odoo_change_value.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_ansible_ansible_add_two_sequence_items.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_ansible_ansible_double_quote_scalar_change_possible_treesitter_weakness.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_axios_axios_update_string_value.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_junegunn_fzf_version_upgrade.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_mastodon_mastodon_add_block_pair_translation.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_mastodon_remove_one_pair.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_mozilla_pdf_single_value_update.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_n8n_io_n8n_add_performance_check_to_ci_config.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_twbs_bootstrap_remove_v_semicolon_from_version_numbers.rs
- mapping
- invariants
### src/test/fixtures/small/yaml_twbs_bootstrap_version_pin_with_comment.rs
- mapping
- invariants
### src/test/fixtures/stratified/c_ffmpeg_ffmpeg_rename_two_identifiers.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_freeciv_freeciv_rename_identifier.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_freeciv_freeciv_update_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_genymobile_scrcpy_add_a_define.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_genymobile_scrcpy_add_a_define_.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_genymobile_scrcpy_add_a_define_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_genymobile_scrcpy_add_to_import_path_and_move_imports_around.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_genymobile_scrcpy_big_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_genymobile_scrcpy_rename_and_add_a_define.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_genymobile_scrcpy_rename_defines.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_godotengine_godot_add_two_enum_values.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_godotengine_godot_pure_move.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_godotengine_godot_whitespace_only.rs
- mapping
- invariants
### src/test/fixtures/stratified/c_htop_dev_htop_add_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_htop_dev_htop_add_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_htop_dev_htop_update_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_ladybirdbrowser_ladybird_change_to_a_different_class.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_ladybirdbrowser_ladybird_move_to_a_different_class.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_ladybirdbrowser_ladybird_small_parse_errors.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_microsoft_terminal_add_two_includes.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_microsoft_terminal_change_import_path.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_mozilla_firefox_firefox_remove_two_comments.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_neovim_neovim_add_an_include.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_neovim_neovim_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_nginx_nginx_add_preproc_two_lines.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_ollama_ollama_change_imports.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_openssl_openssl_add_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_openssl_openssl_add_two_clang_comments.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_openssl_openssl_copyright.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_openssl_openssl_format_only_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_openssl_openssl_identifier_to_literal_zero.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_openssl_openssl_whitepsace_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_openssl_openssl_whitespace_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_copyright.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_copyright_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_copyright_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_copyright_year_update.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_copyright_year_update_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_copyright_year_update_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_fix_typo.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_preprocessor_heavy_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_tiny_but_interesting.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_postgres_postgres_update_copyright_year.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_protocolbuffers_protobuf_add_to_preproc_define.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_protocolbuffers_protobuf_change_regex.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_redis_redis_one_line_insert.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_rust_lang_rust_add_two_consts.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_sqlite_sqlite_fix_format_string_typo.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/c_tmux_tmux_delete_one_include.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_electron_electron_add_imports.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_godotengine_godot_add_include.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_godotengine_godot_add_one_include.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_godotengine_godot_add_one_include_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_godotengine_godot_add_one_include_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_godotengine_godot_add_one_include_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_godotengine_godot_add_one_include_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_godotengine_godot_insert_one_include.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_godotengine_godot_two_imports_added.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ladybirdbrowser_ladybird_add_real_logic.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ladybirdbrowser_ladybird_change_inherited_class_name.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_libreoffice_add_const_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_libreoffice_add_imports_and_function_param.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_libreoffice_delete_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_libreoffice_remove_two_wrapping_functions.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_libreoffice_warn_to_info.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_microsoft_terminal_add_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_microsoft_terminal_delete_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_microsoft_terminal_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_microsoft_terminal_remove_unary_expression_from_binary.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_mozilla_firefox_firefox_delete_leading_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_mozilla_firefox_firefox_update_file_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_mozilla_firefox_firefox_update_file_comment_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_nzbgetcom_nzbget_add_include.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_nzbgetcom_nzbget_update_string_const.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash_6.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash_7.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash_8.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_ollama_ollama_update_commit_hash_string_constant.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_opencv_opencv_delete_string_const_from_preprocessor.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_paddlepaddle_paddleocr_add_namespace_closing_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_protocolbuffers_protobuf_add_preprocessor_commands.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_protocolbuffers_protobuf_one_line_insert.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_pytorch_pytorch_two_inserted_lines.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/cpp_tensorflow_tensorflow_new_to_make_unique.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_jellyfin_jellyfin_add_const_string.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_jellyfin_jellyfin_add_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_jellyfin_jellyfin_update_version_string.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_lidarr_lidarr_add_enum_value.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_lidarr_lidarr_add_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_lidarr_lidarr_add_function_call_to_return.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_lidarr_lidarr_add_function_signature_to_class.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_lidarr_lidarr_add_import_and_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_lidarr_lidarr_add_method_to_class.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_radarr_radarr_add_base_class.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_radarr_radarr_add_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_radarr_radarr_add_one_using.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_radarr_radarr_add_one_using_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_radarr_radarr_add_property.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_radarr_radarr_add_to_end_of_regex.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_radarr_radarr_remove_import_and_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_add_attribute.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_add_attribute_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_add_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_add_import_and_annotation.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_add_list_item.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_add_two_attributes.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_add_two_items_to_list.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_delete_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_fix_comment_typo.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_update_regex.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/csharp_sonarr_sonarr_use_a_different_class.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_facebook_react_add_two_selectors.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_gorhill_ublock_add_one_attribute.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_mastodon_mastodon_add_two_lines.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_microsoft_vscode_remove_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_nextcloud_server_update_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_shadcn_ui_ui_add_two_bytes.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_twbs_bootstrap_parse_errors.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_change_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_go_to_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_one_line_to_multiline.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_re_format_in_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_reformat_and_fix_lint_errors.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_remove_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_remove_one_rule.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_remove_webkit_prefix.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/css_wordpress_wordpress_rename_attribute.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_caddyserver_caddy_multiple_solutions_interesting_case.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_fatedier_frp_build_comment_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_one_space_removed_in_a_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_replace_replaced_with_replaceall.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_update_version_string.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_update_version_string_.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_update_version_string_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_update_version_string_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_update_version_string_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_update_version_string_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_update_version_string_6.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gin_gonic_gin_whitespace_in_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gohugoio_hugo_add_and_upadate_list_items.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gohugoio_hugo_update_and_add_list_items.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gohugoio_hugo_update_and_add_some_values.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_gohugoio_hugo_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_golang_go_add_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_golang_go_update_copyright_year.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_grafana_grafana_real_small_change_with_a_move.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_jesseduffield_lazygit_add_a_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_jesseduffield_lazygit_add_two_lines.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_junegunn_fzf_real_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_ollama_ollama_add_go_build_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_ollama_ollama_remove_go_build_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/go_prometheus_prometheus_remove_copyright_year.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_apache_echarts_delete_one_script_tag.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_axios_axios_add_script_element.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_caddyserver_caddy_add_one_element.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_fatedier_frp_update_hashes.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_fatedier_frp_update_hashes_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_fatedier_frp_update_hashes_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_fatedier_frp_update_hashes_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_fatedier_frp_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_gohugoio_hugo_change_template_variable_path.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_gohugoio_hugo_template_not_pure_html.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_gohugoio_hugo_template_not_pure_html_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_gohugoio_hugo_update_href_template.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_gorhill_ublock_add_one_meta_element.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_ladybirdbrowser_ladybird_remove_meta_attribute.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_ladybirdbrowser_ladybird_remove_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_ladybirdbrowser_ladybird_update_pixel_value.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_ladybirdbrowser_ladybird_update_two_pixel_numbers.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_mozilla_firefox_firefox_href_path.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_mozilla_firefox_firefox_interesting_case.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_mozilla_firefox_firefox_path.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_mozilla_firefox_firefox_test_span.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_mozilla_pdf_add_closing_tags.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_pandas_dev_pandas_release_banner_update.rs
- mapping
- invariants
### src/test/fixtures/stratified/html_prettier_prettier_not_pure_html_includes_yaml_as_well.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_twbs_bootstrap_not_html_template_extract_two_vars.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/html_twbs_bootstrap_remove_one_line_in_yaml_metadata_preamble.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_genymobile_scrcpy_add_enum_value.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_genymobile_scrcpy_add_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_genymobile_scrcpy_add_func_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_genymobile_scrcpy_add_parameter.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_genymobile_scrcpy_char_to_string_bugfix.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_genymobile_scrcpy_only_insert.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_genymobile_scrcpy_whitespace_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_nextcloud_android_add_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_paddlepaddle_paddleocr_whitespace_only.rs
- mapping
- invariants
### src/test/fixtures/stratified/java_paddlepaddle_paddleocr_whitespace_only_2.rs
- mapping
- invariants
### src/test/fixtures/stratified/java_protocolbuffers_protobuf_add_one_annotation.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_protocolbuffers_protobuf_add_one_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_protocolbuffers_protobuf_update_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/java_protocolbuffers_protobuf_update_comment_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_axios_axios_real_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_d3_d3_nice_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_facebook_react_delete_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_facebook_react_update_comment_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_add_use_strict.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_add_use_strict_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_concat_to_template.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_refactor.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_remove_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_small_change_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_small_change_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_use_strict.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_use_strict_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_use_strict_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_use_strict_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_use_strict_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_use_strict_6.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_use_strict_7.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_microsoft_typescript_use_strict_8.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_mozilla_firefox_firefox_remove_one_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_mui_material_ui_delete_one_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/javascript_vercel_next_add_doccomment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_apache_superset_js_to_ts.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_gorhill_ublock_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_grafana_grafana_add_pair.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_lidarr_lidarr_remove_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_microsoft_playwright_version_update.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_puppeteer_puppeteer_update_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_puppeteer_puppeteer_version_update.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_puppeteer_puppeteer_version_update_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_shadcn_ui_ui_add_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/json_vercel_next_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_mozilla_firefox_firefox_add_one_annotation.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_mozilla_firefox_firefox_add_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_mozilla_firefox_firefox_add_one_line_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_mozilla_firefox_firefox_rename.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_add_one_annotation.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_add_param.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_add_param_to_class.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_different_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_real_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_remove_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_remove_one_argument.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_remove_one_argument_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_remove_one_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_rename.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_rename_field.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_nextcloud_android_whitespace_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/kotlin_rustdesk_rustdesk_add_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_add_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_add_deprecation_notice_to_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_add_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_add_func_call.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_add_h_to_align.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_add_noreturn_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_add_to_table_constructor.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_comment_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_comment_only_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_fix_one_byte_typo_in_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_halign.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_insert.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_awesomewm_awesome_update_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_neovim_neovim_add_leading_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/lua_neovim_neovim_rename.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_a_few_types.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_declare.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_declare_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_declare_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_declare_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_declare_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_declare_6.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_declare_7.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_one_element_to_array_init.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_add_readonly.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_nextcloud_server_real_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_add_one_require_statement.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_not_sure_if_this_parses_correctly.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_one_line_file_insert_and_update.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_one_line_file_with_real_insert_and_update.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_remove_one_todo_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_version_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_version_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_version_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_version_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_version_6.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_version_7.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/php_wordpress_wordpress_version_8.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_ansible_ansible_add_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_langchain_ai_langchain_version_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_langflow_ai_langflow_bob.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_langflow_ai_langflow_real_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_nvbn_thefuck_add_three_arguments.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_nvbn_thefuck_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_nvbn_thefuck_small_change_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_nvbn_thefuck_small_change_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_nvbn_thefuck_stdout_stderr_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_odoo_odoo_add_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_odoo_odoo_add_import_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_odoo_odoo_add_two_imports.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_odoo_odoo_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_openhands_openhands_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_paddlepaddle_paddleocr_remove_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_paddlepaddle_paddleocr_whitespace_only.rs
- mapping
- invariants
### src/test/fixtures/stratified/python_paddlepaddle_paddleocr_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/stratified/python_scrapy_scrapy_comment_update.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_ytdl_org_youtube_dl_version_update.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/python_ytdl_org_youtube_dl_version_update_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_homebrew_brew_add_extends.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_homebrew_brew_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_version_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_version_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_version_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_version_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_version_6.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_version_7.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_whitespace_only.rs
- mapping
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_whitespace_only_2.rs
- mapping
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_whitespace_only_3.rs
- mapping
- invariants
### src/test/fixtures/stratified/ruby_jekyll_jekyll_whitespace_only_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_add_func_and_attribute.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_add_method.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_add_one_call.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_add_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_move.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_normal_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_one_operator.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_rare_example_of_true_move.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/ruby_mastodon_mastodon_smal_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_gyulyvgc_sniffnet_add_mod.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_gyulyvgc_sniffnet_add_mod_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_gyulyvgc_sniffnet_add_mod_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_gyulyvgc_sniffnet_remoev_attribute.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_gyulyvgc_sniffnet_rename_one_identifier.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_protocolbuffers_protobuf_add_enum_variant.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_protocolbuffers_protobuf_add_two_attributes.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_add_note_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_add_one_check.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_add_or_expression.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_change_use.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_one_comment_line_into_two.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_remove_min_version_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_remove_path_from_using.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_remove_starting_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_remove_warn_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_remove_warn_comment_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_remove_warn_comment_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_remove_warn_comments.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rust_lang_rust_update_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rustdesk_rustdesk_add_one_token.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_rustdesk_rustdesk_move_list_item_with_insert.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_tauri_apps_tauri_add_doccomment_empty_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_tauri_apps_tauri_add_path_to_const_string.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_tauri_apps_tauri_add_pub_to_mod.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_tauri_apps_tauri_add_to_list.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_tauri_apps_tauri_add_use_and_function.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_tauri_apps_tauri_fix_typo_in_string_constant.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_tauri_apps_tauri_rename_mod.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_vercel_next_add_mode.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_vercel_next_remove_mod.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_vercel_next_simple_identifier_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_zed_industries_zed_add_mod.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_zed_industries_zed_change_mod_and_use.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/rust_zed_industries_zed_change_mods.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_ansible_ansible_a_small_add.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_ansible_ansible_add_commadn.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_ansible_ansible_only_insert.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_ansible_ansible_small_add.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_ansible_ansible_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_genymobile_scrcpy_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_genymobile_scrcpy_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_genymobile_scrcpy_version_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_jesseduffield_lazygit_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_microsoft_playwright_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_mongodb_mongo_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_nvm_sh_nvm_add_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_openhands_openhands_update_string_value.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_paddlepaddle_paddleocr_insert_inside_a_string.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_pandas_dev_pandas_remove_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_pi_hole_pi_hole_add_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_scikit_learn_scikit_learn_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_stgpetrovic_stacuist_pure_add.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_stgpetrovic_stacuist_pure_add_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/shellscript_vercel_next_change_command_params.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_nextcloud_ios_add_one_log_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_nextcloud_ios_different_func.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_add_target_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_add_to_typecheck_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_add_to_typecheck_comment_1.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_add_to_typecheck_comment_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_add_to_typecheck_comment_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_add_to_typecheck_comment_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_add_to_typecheck_comment_6.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_add_to_typecheck_comment_7.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_constraint_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_delete_and_insert_in_the_typecheck_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_insert_delete_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_signature_next_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_signature_next_comment_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_small_change_mostly_add.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_target_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_target_comment_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_update_leading_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/swift_swiftlang_swift_update_typecheck_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_excalidraw_excalidraw_add_type_to_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_langflow_ai_langflow_add_type_to_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_langflow_ai_langflow_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_langflow_ai_langflow_split_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_langflow_ai_langflow_split_import_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_langflow_ai_langflow_split_import_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_microsoft_typescript_delete_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_microsoft_typescript_libpath_to_lib.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_add_attribute.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_add_to_empty_block.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_delete_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_move_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_remove_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_remove_import_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_remove_import_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_remove_import_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_mui_material_ui_remove_one_import.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/tsx_shadcn_ui_ui_order_of_class_names.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_es_target.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_es_target_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_es_target_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_es_target_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_es_target_5.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_es_target_6.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_eslint.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_lib_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_module_and_target_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_one_item.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_one_line.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_strict.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_strict_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_strict_false.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_target.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_target_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_target_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_target_4.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_add_target_comment_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_expand_target.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_expand_target_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_expand_target_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_microsoft_typescript_extend_target_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/typescript_vercel_next_whitespace_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_add_one_dict_entry.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_add_one_dict_item.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_add_one_line_to_dict.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_add_one_line_to_test.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_comment_only_insert.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_date_update_plus_bugfix.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_expand_author_comment.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_insert_only_3.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_only_delete.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_small_change.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/vimscript_neovim_neovim_small_change_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_genymobile_scrcpy_remove_package_attribute.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_godotengine_godot_update_element_value.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_jellyfin_jellyfin_update_attribute_values.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_libreoffice_add_one_menu_item.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_libreoffice_unicode.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_microsoft_terminal_add_one_element.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_mozilla_firefox_firefox_update_value.rs
- mapping
- invariants
### src/test/fixtures/stratified/xml_nextcloud_android_add_one_element.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_nextcloud_android_add_translation.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_nextcloud_android_add_translation_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_odoo_odoo_add_attribute_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_odoo_odoo_insert_only.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/xml_paddlepaddle_paddleocr_whitespace_only.rs
- mapping
- invariants
### src/test/fixtures/stratified/xml_paddlepaddle_paddleocr_whitespace_only_2.rs
- mapping
- invariants
### src/test/fixtures/stratified/xml_paddlepaddle_paddleocr_whitespace_only_3.rs
- mapping
- invariants
### src/test/fixtures/stratified/xml_paddlepaddle_paddleocr_whitespace_only_4.rs
- mapping
- invariants
### src/test/fixtures/stratified/xml_paddlepaddle_paddleocr_whitespace_only_change.rs
- mapping
- invariants
### src/test/fixtures/stratified/yaml_ansible_ansible_add_block_sequence.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_ansible_ansible_add_item_to_sequence.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_ansible_ansible_add_item_to_sequence_2.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_ansible_ansible_add_mapping_pair.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_ansible_ansible_change_in_string_scalar.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_ansible_ansible_rename_string_scalar.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_ansible_ansible_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_gyulyvgc_sniffnet_version.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_jekyll_jekyll_true_to_false.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_mastodon_mastodon_delete_one_pair.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_mastodon_mastodon_remove_one_translation.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_mastodon_mastodon_remove_translation.rs
- mapping
- painting
- invariants
### src/test/fixtures/stratified/yaml_puppeteer_puppeteer_false_to_true.rs
- mapping
- painting
- invariants
### src/test/helper.rs
- repository slug matches the clone directory name sample csv records
- upstream commit url needs a commit and a resolvable repository
- the corpus provenance resolves to upstream urls
- a note round trips and a blank one deletes the file
- a name no dataset holds has no note path
- a multi line note becomes one csv line
- the descriptions already in the corpus are readable
- node for path
- find first of kind includes the starting node and searches depth first
- a path segment splits on its last colon
- precompute paths agrees with path for node
- path for node round trips through node for path
- path cache resolve matches node for path for every node
- path to repo path
- handmade code contains hello world
- handmade test code as paths
- handmade test case dirs lists every diff
- handmade test code pairs no change diff
- entire path has mapping
### src/test/helper/human_mapping.rs
- span text uses byte columns and an exclusive end
- a match derives move from identical text and update from differing text
- a match may be n to m when each side reads the same
- an n to m match whose sides read the same is a move
- a match whose spans disagree within one side is rejected
- a delete may cover several spans
- a malformed entry is an error rather than a default
- a mapping without a text painting serializes without the key
- a preset collects every alternative named for it
- a preset name must be the whole name or be followed by a qualifier
- a single painting answers for either preset whatever it is called
- a preset with no painting named for it says which names the fixture has
- a named but empty painting is distinguishable from no painting at all
- text mapping disagreements reports nothing when the two accounts agree
- text mapping disagreements finds text only one account calls changed
- round trips through json
- deserializes legacy json with no groups key as empty groups
- serializing a mapping with no groups omits the groups key
- resaving an existing fixture produces byte identical json
- round trips a multi map group through json
- line disagreement count counts positions where the two slices differ
- unix diff line labels marks only the changed line on each side
- unix diff line labels marks nothing for identical files
- line mismatches for is zero for a fixture codediff solves exactly
- rebuild caches distinguishes identical from update and match but not identical
- rebuild caches flags an identical match at a different path as moved
- detects a correct hand written mapping for rust no change
- detects an incorrect hand written mapping
- an all to all group round trips and a default pairing leaves the file unchanged
- representative entries pairs every member of an all to all group
- check group entry reports each all to all member a one to one diff leaves out
- check group entry accepts a one to one diff that pairs every all to all member
- check group entry closes an all to all group over the union of its members
- check group entry passes for a real diff that matches duplicates within the group
- check group entry fails when codediff deletes and inserts instead of matching
- check group entry fails when a member matches outside the group
- check group entry with children fails when a descendant leaks outside the matched subtree
- check group entry with children fails when a leftover members descendant is not deleted
- representative entries pairs equal sized groups by start byte
- representative entries puts the surplus before nodes on delete with children
- representative entries puts the surplus after nodes on plain insert
- rebuild caches for mapping reports group membership and status for every member
- rebuild caches for mapping keeps plain entries when a group does not resolve
- unmarked node count of an empty mapping is every node
- as ast diff for mapping projects a group through its representative pairing
- describe path map differences is empty when runs agree
- describe path map differences reports a differing operation
- describe path map differences reports a pair missing from one run
- describe nondeterminism is empty for stable source
- node extents matches the total node count denominator
- node extents marks a real subset as leaves
- node extents is empty without an ast
- nodes touched by marks only overlapping nodes
- a pair with no grammar paints from the plain text fallback
### src/test/helper/human_mapping/invariants.rs
- identifier words split on case and underscores
- a pure prefix insertion paints only the new words
- a single word identifier accepts the bare affix or the whole token
- a shared run inside a word does not split it
- a byte moved under one preset and deleted under the other is reported
- a move widened into an update is not a contradiction
- a minimal consistent with some full alternative is not reported
- two ranges claiming the same byte are reported
- two ranges meeting at a line break do not overlap
- a crlf break between two ranges is not an overlap
- two paintings may claim the same byte as each other
- a painted run that ends on a trailing space is reported
- a painted run that ends on the last visible character is accepted
- a painted run that ends on a mid row space is accepted
- a painted run that is entirely whitespace is exempt
- a row with nothing visible on it is exempt
- a minimal painting reaching past its full counterpart is reported
- a full painting wider than its minimal counterpart is the expected shape
- a single painting answers for both presets so there is nothing to compare
- a painting that marks one half of a pair is not a violation
- only a leaf whose kind is its own text is a delimiter
- a painted move over a node the mapping deletes is reported
- a painted move over a node the mapping inserts is reported
- a painted move the renderer deletes from inside a matched node is not reported
- unmatched bytes lets a matched node override the subtree deleted around it
- a painted delete over a node the renderer calls moved is not reported
- a painted move the mapping calls updated is not reported
- a fixture with no tree mapping is skipped
- a deleted paren whose partner is matched is reported
- a delimiter whose group leaves it free contradicts nothing
- an all to all group member is a claim under every reading
- a delimiter its group pins is still checked
- a mapping that says nothing about a delimiter contradicts nothing
- a pair with a parse error between its halves is skipped
- the delimiter table pairs every opener with at least one closer
- a paired leaf painted gone and new is reported
- a paired leaf painted gone on one side only is not reported
- a removed named leaf nobody painted is reported and punctuation is not
- a removed leaf with one painted byte is not reported
- an edited leaf painted on neither side is reported
- an edited leaf painted on one side is enough
- a painted edit over an all identical mapping is reported
- a whitespace only painting over an all identical mapping is not reported
- an identical entry over differing tokens is reported
- an identical entry over a whitespace difference is not reported
- a match but not identical over byte identical subtrees is reported
- a match but not identical whose subtrees differ is the expected shape
### src/test/helper/human_mapping/tests/exploratory.rs
- mapping vs painting disagreement detail for fixture (skipped)
- measure stub fixtures (skipped)
- painting disagreement detail (skipped)
- invariant violations (skipped)
- mismatch detail for fixture (skipped)
- mismatch census (skipped)
- cross fixture convention census (skipped)
- painting failure census (skipped)
### src/tui.rs
- log file prefers xdg state home then home then the temp dir
### src/tui/app.rs
- panic message extracts str payload
- panic message extracts string payload
- panic message falls back for unknown payload type
- select file for panel only starts diff once both sides are set
- compute diff treats dev null before as an empty file in the afters language
- compute diff treats dev null after as an empty file in the befores language
- compute diff falls back to plain text when neither side has a recognizable language
- open files loads both panels and starts diff
- opening a reviewed file sets the position and stepping walks its set
- a binary reviewed file is a banner not a crash and stepping moves past it
- picking a binary file by hand is a banner not a crash
- opening the review outside a repository goes to the banner
- esc should quit is false for every screen with its own dialog
- q quits only from the viewer and the diffing wait
- esc should quit is true only on the bare viewer
- a stale diff computed result is dropped after cancel
- ctrl z is recognised as suspend and a bare z is not
- handle dialog cancelled resets dialog state
- cancelling the render options panel restores what it opened with
- apply render options reloads when whole pair updates changes
- apply render options reloads for every construction time field
- apply render options does not reload for the other fields
- apply render options with no open pair does not queue a reload
- redelivering the opening keystroke would immediately close the help modal
- handle search submitted jumps the focused panel and returns to the viewer screen
- submitting an empty search repeats the last submitted query
- recent pairs are offered only while no file is loaded
- redelivering the opening keystroke would seed the search query with a stray slash
- handle file selected loads into the dialogs target panel
- accepting the render options panel keeps what it applied
- apply theme selection updates viewer and returns to the viewer screen
- draw viewer badges minimal options in the footer
- draw viewer does not badge full options
- draw viewer badges a single disabled option by name
- draw viewer shows the diff summary status bar when set
- draw viewer shows no status bar when diff summary is none
- the diffing screen counts elapsed time in tenths
- the diffing screen omits the counter when no diff is timed
- the diff clock is cleared when the diff finishes
- draw viewer shows a centered toast while the countdown runs
- summary toast fades on its own but the status bar stays
- summary toast is dismissed without waiting for the countdown
- no toast is drawn when the diff has no summary worth reporting
- diff ready starts the toast countdown
- summary toast area is centered and clamped to a narrow terminal
- draw viewer always shows the footer key hints
- format change counts omits zero categories
- draw viewer shows change counts in the footer when set
- draw viewer shows change progress in the footer once the focused panel has changes
- draw viewer shows search match progress in the footer in place of change progress
- draw viewer shows plain text fallback in the footer
- draw viewer shows both the status bar and the error banner at once
- diff ready summary matches summarize diff on the same session data
- diff ready summary reports comment only when the session data says so
- compute diff reports comment only for a real inserted comment
- compute diff never puts a raw tab into diff session data contents
- compute diff leaves a crlf file offset stable and free of other control bytes
- display safe leaves multi byte control code points alone
- display safe keeps a crlf terminator and substitutes a lone carriage return
### src/tui/components/code_viewer.rs
- non whitespace bounds finds the first and last non whitespace columns
- clamp to non whitespace pulls back from trailing whitespace
- clamp to non whitespace pushes forward from leading whitespace
- clamp to non whitespace leaves a column within bounds untouched
- clamp to non whitespace falls back to the plain clamp on an all whitespace line
- move cursor vertical remembers the desired column across a shorter line
- move cursor vertical clamps past the end of line to the last non whitespace column
- move cursor vertical clamps before the first non whitespace column
- move cursor vertical does not panic on an all whitespace line
- move cursor horizontal resets the sticky column
- set cursor position resets the sticky column
- move cursor horizontal right advances within a line
- move cursor horizontal right wraps to the start of the next line
- move cursor horizontal right is a no op at the very end of the file
- move cursor horizontal left retreats within a line
- move cursor horizontal left wraps to the end of the previous line
- move cursor horizontal left is a no op at the very start of the file
- move cursor horizontal with zero direction is a no op
- search jumps the cursor to the first match at or after the cursor
- search with no matches leaves the cursor untouched
- jump to search match steps forward and wraps
- search match count and index reflects the active search
- jump to change centers the destination row
- jump to change clamps to the start of the file
- jump to change clamps to the end of the file
- change bands marks each band a change touches and skips identical ranges
- change bands does not count the row a range ends on at column zero
- change bands shows the highest priority operation in a shared band
- cursor screen position is none once the cursor scrolls out of view
### src/tui/components/diff_viewer.rs
- switching render options keeps the cursor where it is
- loading a diff still jumps to the first change
- tab moves focus exclusively to the other panel
- draw dual panel shows each filename exactly once
- draw never draws a border around either panel in either mode
- focused cursor position is none before any file is loaded
- focused cursor position reflects the active panels cursor
- moving cursor on active side moves inactive side cursor to matched node
- jump to change skips unchanged lines and syncs the other panel
- jump to change centers both panels
- n crosses to the other panel when that is where the next change is
- the change counter reports the merged total
- p crosses to the other panel too
- crossing panels lands on the change
- jumping with no changes anywhere does nothing
- search jumps the focused panel and syncs the other panel
- jump to search match steps through matches on the focused panel
- focused search match count and index is none before any search
- node highlight is off by default and h enables both panels
- change stops visit a paired change once on the before side
- a deletion is visited before the insertion that replaces it
- enter jumps to the counterpart and back
### src/tui/components/file_dialog.rs
- typing narrows the listing and dotfiles are hidden by default
- ctrl h toggles hidden files
- backspace widens the filter before falling back to parent navigation
- enter selects from the filtered view not the raw listing
- arriving directory listing clears the filter
### src/tui/components/help_modal.rs
- question mark and esc both close the modal
- j and k scroll down and up
- scroll does not go negative
- help modal renders keybindings
- help modal renders a legend with the current themes backgrounds
### src/tui/components/line_prompt.rs
- digits accumulate and submit as a line number
- non digit characters are ignored entirely
- backspace edits and empty enter cancels
- esc cancels
- zero is rejected as a cancel not a jump
### src/tui/components/render_options_dialog.rs
- space toggles the selected option and reports it
- down then space toggles the second option
- digits jump straight to the named presets
- enter accepts and changes nothing on the way out
- the key that opens the panel is inert inside it
- esc cancels without changing anything
- esc after a change still reports what the panel opened with
- selection does not move past the last row
- popup is wide enough for the whole hint
### src/tui/components/review_dialog.rs
- rows list the three sections with the newest commit unfolded
- navigation skips headers and notes and stops at the ends
- enter on a file reports the target and its index within its set
- enter and arrows fold and unfold a commit
- esc and g close the dialog
- an empty repository shows its notes and selects nothing harmful
- draws the root and every row
### src/tui/components/search_modal.rs
- typing characters builds up the query
- backspace removes the last character
- backspace on an empty query is a no op
- enter submits the typed query
- esc cancels without submitting
- cursor screen position follows the typed query
### src/tui/components/theme_dialog.rs
- new preselects the current theme and loads its colors
- cycling the theme reloads the color rows
- editing a color switches the selection to custom
- an unparseable hex value is rejected
- esc while editing cancels the edit not the dialog
- enter on a dropdown row accepts the dialog
- navigation stops at both ends
- every color slot round trips through the palette
- renders every row with a value
- hex round trips
### src/tui/headless.rs
- lines to keep keeps context lines around a change and nothing else
- lines to keep merges context windows of nearby changes
- render side collapses a long run of unchanged lines
- row overlay does not color a middle rows trailing whitespace
- a highlight covers the same text on ascii and non ascii rows
- colorizing never alters the text of the row
- render side wraps a moved chunk in a box with header bar and footer
- nearest reference line finds the enclosing function not the nearest if
- render side shows the enclosing function as a breadcrumb when out of context
- render text diff without color shows both sides with markers
- render text diff with color highlights only the changed substring inline
- run prints a readable diff for two real files
- render text diff omits the summary header for an ordinary mixed edit
- render text diff shows a no changes header for identical files
- render text diff shows a comment only header when only a comment changed
- render text diff bolds the summary header when colored
- update marker wins over insert and delete on the same line
- render side strips the carriage return of a crlf row
- render side omits the breadcrumb when the enclosing line is already shown
- run reports a tab versus spaces difference
### src/tui/json_output.rs
- build diff omits identical ranges and keeps only the real change
- build diff never sets move target for non move operations
- build diff sets move target only for a move operation
- build diff carries the large residual flag through as a field
- build diff omits summary for an ordinary mixed edit
- build diff sets summary to no changes for identical content
- build diff omits the summary field entirely from serialized json when none
- run prints valid json with the expected top level shape
- build side sets reference line to the enclosing function
- binary diff json keeps language but has no hunks or summary
- text diff json omits the binary field
### src/tui/screenshot.rs
- render fills every cell and paints the change
- overlay themes resolve by label or variant name
- indexed colours follow the xterm table
### src/tui/theme.rs
- an unknown theme name does not discard the rest of the config
- a file that does not parse is never overwritten
- the environment override wins over every other layer
- recent pairs drops entries whose files are gone
- a throwaway vcs path is recognised
- a project config is found from a subdirectory
- the nearest project config wins
- no project config means none is created
- saving creates the directories the user config lives in
- the user config path follows xdg then home
- save then load round trips the chosen theme
- a pre existing render options table without whole pair updates still loads
- node highlight round trips and defaults to off for an older config
- load from a missing file falls back to default without erroring
- blend toward base interpolates correctly
- every added theme is visually distinct from dark
- every themes move band is grey rather than a hue
- every theme has visually distinct bands
- every themes search color is distinct from bands and cursor highlight
- panel layout cycle visits all three modes and returns
- saving one setting preserves the other
- an unparseable custom color falls back to that dracula color only
- parse hex color accepts a bare hex and rejects other shapes
- a named ansi color formats as its xterm value and forks to rgb
### src/tui/ui.rs
- maps key event
- drops key release events but keeps presses and repeats
- restore terminal is a no op outside raw mode
- maps resize event
- drops focus and paste events
- maps mouse event
### src/tui/widgets/code_viewer.rs
- syntax highlighting is enabled by default and actually highlights
- every language except the documented bazel gap resolves to a real syntax
- set theme with an unknown name falls back instead of panicking
- find matches finds case insensitive occurrences in document order
- find matches is empty for an empty query or no occurrences
- find matches columns are correct when lowercasing changes character count
- a search highlight covers the query on a non ascii row
- diff overlay pairs explicit foreground with background
- multi row range does not paint a middle rows trailing whitespace
- node highlight is off until enabled
- cross highlight is suppressed for an identical match
- cursor overlay uses bright blue with explicit foreground
- unfocused panel does not highlight its own cursor range
- focused panel ignores stale highlight destination
- node highlight off leaves the cursor range showing its diff color
- node highlight off leaves the counterpart unpainted
- range at finds covering range and resolves ties and gaps
- load ranges places cursor on first navigable position
- next change position finds the next change forward
- next change position finds the previous change backward
- next change position wraps around at the ends
- next change position is none when the file has no changes
- change count and index is none when the file has no changes
- change positions collapses a multi row insert split into one stop
- change positions does not collapse adjacent but unrelated changes
- change count and index counts changes at or before the cursor
- nearest search match position finds the match at or after the cursor
- nearest search match position is none with no matches
- next search match position wraps around at the ends
- search match count and index counts matches at or before the cursor
- search match count and index is none with no matches
- load ranges clears any previous search matches
- overlay row paints search matches in the dedicated search color
- cursor destination returns matched range for current position
- cross highlight destination uses bright blue with explicit foreground
- set overlay theme changes painted colors
- render never draws its own border or title
- gutter width tracks line count and disappears when empty
- slice columns windows a line and marks cut edges
- find matches is case sensitive when the query has an uppercase letter
- paint columns reads columns as bytes on a multi byte row
- trailing whitespace trimmed len counts bytes and drops trailing whitespace
- replace ranges keeps the cursor and search but drops the cross highlight
### src/web/http.rs
- a get with headers and no body parses
- a post reads exactly content length bytes
- a closed connection before any bytes is not an error
- a head cut off before the blank line is malformed
- bare newlines are tolerated
- a body over the bound is refused before it is read
- an oversized head is refused
- chunked bodies are rejected rather than misread
- parse head rejects the shapes it does not understand
- a response serializes with the headers the page relies on
- json errors have the one shape the page shows
### src/web/payload.rs
- utf16 columns count code units not bytes
- ranges convert each end against its own row
- highlighting covers a rust line in utf16 columns with merged runs
- a language without a syntax yields empty spans not an error
- an unknown syntax theme falls back to syntects default
- a dev null side reports the other sides language
- the diff payload carries lines ranges counts and the syntax theme
- identical files summarize as no changes
- the highlight payload only reswaps spans
- serialized spans are compact triples
### src/web/server.rs
- host header must name this server at its port
- tokens are long and differ between calls
- the page carries the placeholder the server fills
- the pages footer hints are the tuis
- the index is served with the token and the api demands it back
- a diff round trips and quit stops the server
### src/web/session.rs
- render option rows match the structs own serialization order
- the state names every theme with a round trippable id
- a stale result is dropped and a current one stored
- cancelling retires the in flight generation but keeps the last result
- a failed diff reports its message
- a filter only option change refilters without a new diff
- a construction time option change asks for a recompute
- settings persist through the same config the tui reads
- a custom palette is installed as well as saved
- highlight is none until a pair is loaded
- listing puts directories first and the parent on top
- listing an unreadable directory still offers the way out
- a reviewed file is materialized and diffed like any pair
- path for names the two panels and nothing else
### src/web_main.rs
- no flags means the persisted options
- a preset flag is the preset
- the single option flags layer onto a preset
- minimal and full conflict
- git argument shapes are accepted
### tests/benchmark_other_e2e.rs
- nothing changed and everything changed are exact complements
- an unsupported language is skipped rather than scored
- a random answer is reproduced exactly by a second run
- a random answer is neither degenerate case
- a failing tool is recorded as an error without stopping the run
- the scoping flags narrow the run
- an unknown tool name is rejected
- an unknown fixture name is rejected

## Named by no test
- src/test/fixtures.rs — defects4j, stratified, handmade
- research/analysis/paper_variables.py — robustness_fixtures, common_subset_concentration, robustness_full, sampling_provenance
- research/analysis/verify_sample.py — resolves
- src/stats/git.rs — walk_single_parent_commit_diffs, blob_bytes, text_loc_if_in_range
- src/tui/widgets.rs — code_viewer
- src/anomalous_paths.rs — is_anomalous
- research/analysis/file_stats.py — write_paper_variables, export_size_distribution, load_data, load_node_kind_counts
- research/analysis/ambiguity_report.py — summarize
