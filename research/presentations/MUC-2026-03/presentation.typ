#import "@preview/polylux:0.4.0": *
#import "@preview/helios-polylux:0.1.0": *

#bibliography("references.yaml")

#let percentiles = csv("percentiles.csv")

#show: setup.with(colour-accent: rgb("#E27B01"))

// There is no specific function to create a cover slide for the
// presentation. Here is an example for using standard Typst
// functionality to this end:
#slide[
  #set page(header: none, footer: none)
  #set text(stretch: 50%)
  #set par(spacing: 1em)
  #show: pad.with(top: 10%, left: 5%, bottom: 10%, right: 5%)

  #text(size: 1.5em, weight: "bold", stretch: 50%)[
    Things to know when building tools that work on code
  ]

  #text(size: 1em, weight: "medium", stretch: 50%)[
    Why is this not common knowledge?
  ]

  #v(5fr)
  #text(weight: "regular", size: 0.75em)[
    #grid(columns: (1fr, 1fr, 1fr),
      text(weight: "medium", upper[Marko Ivanković#super[1]]),
    )
  ]

  #v(2fr)

  #text(style: "normal", size: 0.75em)[
  #super[1]#link("mailto:marko@ivankovic.me"), Universität Passau
  ]

  #v(5fr)

  #text(size: 0.85em, weight: "medium")[BMW] \
  #text(size: 0.85em)[München, 09-10.3.2026.]

]

#slide[
  = Overview

  #set text(size: 1.25em)

  #outline
]


#make-section[Background]

#slide[
  = Background

  - As an industry, we make a lot of tools for Software Engineers.

  - Many of these tools work on code.

  - But do we know how code _actually_ looks like?
]

#slide[
  = Why does this matter?

  - How often did you see the "worst case" "upper bound" Big-#sym.Omicron notation in papers?

  - As an example, in "Fine-grained and Accurate Source Code Differencing" @gumtreediff-2014, the authors state that the complexity of their algorithm is #sym.Omicron (n#super[2]) where n is the maximum of the number of nodes in the two trees being compared.

  - What _is_ the maximum number of nodes?

    - What is the theorethical maximum number of nodes?

    - What is the physically possible maximum number of nodes?

    - What is the actual maximum number of nodes on the Planet Earth today?

    - What is the maximal number of nodes your tool will ever see?

    - What is the second largest number? Or how about 99.99th percentile?
]

#slide[
  = Difference between _engineering_ and _science_

  - If civil engineers built houses the way we build software, every house would be designed to survive a volcanic eruption happening at the same time as a nuclear blast... underwater becase a tsunami just passed over the area.

  - When you build a house, the first step is to get a geological engineer to do ground analysis and then you design your house based on actual constraints, not based on possible constraints.

  - Very often, we build the tool first, using theorethical "worst case" analysis to guide the developement, and then we measure the actual performance _after_ the tool is built.
]

#make-section[Existing research]

#slide[
  = Two separate papers that use AST nodes to look for API usage

  - "Mining Billions of AST Nodes to Study Actual and Potential Usage of Java Language Features" @ast-api-1 and "Large-scale, AST-based API-usage analysis of open-source Java projects" @ast-api-2 both use AST analysis to find function calls and analyze API usage

  - Neither provides a comprehensive analysis of size, except for offhand sentences like "In this paper, we analyze over 31k opensource Java projects representing over 9 million Java files, which when parsed contain over 18 billion AST nodes"

  - About 2 thousand nodes per file on average then :)

  - Java only
]

#slide[
  = The Commit Size Distribution of Open Source Software @commit-size-distribution-paper.

  #grid(columns: (1fr, 1fr), gutter: 4em)[
    - Best paper I found so far for commit sizes

    - 8M commits across 9363 projects

    - _Key-takeaway_: Distribution of commit sizes is logarithmic.

      - 47.5% of all commits are 10 or fewer lines.

      - Commits beyond 10000 lines do exist.

    - _Some limitations_:

      - Doesn't provide a per-language overview

      - LOC focus, not AST nodes
  ][
    #image("arafat-riehle-commit-sizes.png")
  ]
]

#make-section[Let's dig]

#slide[
  = Let's find the answer to our questions

  - What is the theorethical maximum number of nodes?

    - Ok this is probably #sym.infinity.

  - What is the physically possible maximum number of nodes?

    - There are 10#super[80] atoms in the universe. So if we converted the entire universe into RAM... A lot, but infinitely fewer than #sym.infinity.

  - What is the actual maximum number of nodes on the Planet Earth today?

  - What is the maximal number of nodes your tool will ever see?

  - What is the second largest number? Or how about 99.999th percentile?
]

#slide[
  = What is the actual maximum number of nodes on the Planet Earth today?

  - We can't know this. Too many private repositories. But we can make a decent educated guess.

  - Let's take the top 1000 open source repositories, let's add the roughly 7300 more repositories that make up the Gentoo Linux distribution.

  - Let's clone them all and count.
]

#slide[
  = Attack of the clones

  #grid(columns: (1fr, 1fr), gutter: 4em)[
    - Repositories: 7423

    - Number of files: 6169828

    - Languages: 31

    - 4 _types_ of files
  ][
    #image("plots/language_distribution.png")
  ]
]

#slide[
  = Repositories contain a lot of stuff, not just code

  #grid(columns: (1fr, 1fr), gutter: 4em)[
    - Code
      - This is what you expect to find

    - Configuration
      - Usually human readable. Yaml and similar formats
      - A lot of it is configuring the developement environment

    - Data
      - Images, audio, video...
      - Test data
      - Data stored as very long arrays directly in code

    - Documentation
      - Project guides, e.g. README.md
      - Legal, e.g. licences
  ][
    #image("plots/tips.png")
  ]
]

#slide[
  = Files per project

  #grid(columns: (1fr, 1fr), gutter: 4em)[
    - Median: 93

    - 90th percentile: 1 500

    - 99th percentile: 11 769

    - 99.9th percentile: 49 049
  ][
    #image("plots/files_per_project.png")
  ]
]

#slide[
  = Bytes per file (Code files only)

  #grid(columns: (1fr, 1fr), gutter: 4em)[
    - Median: 2 268

    - 90th percentile: 17 759

    - 99th percentile: 134 118

    - 99.9th percentile: 1 193 619
  ][
    #image("plots/bytes_distribution.png")
  ]
]

#slide[
  = LOC per file (Code files only)

  #grid(columns: (1fr, 1fr), gutter: 4em)[
    - Median: 59

    - 90th percentile: 452

    - 99th percentile: 2 780

    - 99.9th percentile: 12 909
  ][
    #image("plots/lines_of_code_distribution.png")
  ]
]

#slide[
  = AST nodes per file (Code files only)

  #grid(columns: (1fr, 1fr), gutter: 4em)[
    - Median: 332

    - 90th percentile: 3 552

    - 99th percentile: 24 516

    - 99.9th percentile: 138 995

    - _99.99th percentile_: 289 352

    - _Maximum_: 905 004
  ][
    #image("plots/ast_nodes_distribution.png")
  ]
]

#slide[
  = Correlation between bytes and AST nodes (Code files only)

  #grid(columns: (1fr, 1fr), gutter: 4em)[
    - Pearson correlation: 0.8952

    - Formula: $ |"AST"| = "bytes" * 0.2124 + 49.9386 $

    - More useful formula: $ |"AST"| = "bytes" / 5 $

    - _This allows comparing algorithms that operate on bytes and AST nodes!_

      - E.g. O(bytes#super[2]) = O(|AST|#super[2])
  ][
    #image("plots/ast_nodes_bytes_correlation.png")
  ]
]

#slide[
  = Differences between selected languages - Bytes

  #set align(center)

  #table(
   columns: 7,
   [_Language_], [_p50_], [_p90 ↓_], [_p99_], [_p99.9_], [_p99.99_], [_Max_],
   ..percentiles
      .filter(row => row.at(1) in ("C", "CPP", "Go", "Rust", "Java", "Python", "TypeScript", "JavaScript"))
      .filter(row => row.at(0) == "bytes")
      .map(row => row.slice(1))
      .sorted(key: row => float(row.at(2)))
      .flatten(),
  )
]

#slide[
  = Differences between selected languages - Lines

  #set align(center)

  #table(
   columns: 7,
   [_Language_], [_p50_], [_p90 ↓_], [_p99_], [_p99.9_], [_p99.99_], [_Max_],
   ..percentiles
      .filter(row => row.at(1) in ("C", "CPP", "Go", "Rust", "Java", "Python", "TypeScript", "JavaScript"))
      .filter(row => row.at(0) == "lines_of_code")
      .map(row => row.slice(1))
      .sorted(key: row => float(row.at(2)))
      .flatten(),
  )
]

#slide[
  = Differences between selected languages - AST Nodes

  #set align(center)

  #table(
   columns: 7,
   [_Language_], [_p50_], [_p90 ↓_], [_p99_], [_p99.9_], [_p99.99_], [_Max_],
   ..percentiles
      .filter(row => row.at(1) in ("C", "CPP", "Go", "Rust", "Java", "Python", "TypeScript", "JavaScript"))
      .filter(row => row.at(0) == "ast_nodes")
      .map(row => row.slice(1))
      .sorted(key: row => float(row.at(2)))
      .flatten(),
  )
]

#make-section[Rules of thumb]

#slide[
  = Quick rules of thumb when time is precious

  - _Repository composition_: Two-thirds code, one third other stuff.

  - _Files per project_: Tens of thousands mostly, can be hundreds of thousands.

  - _Bytes per file_: Tens of thousands mostly, can be half a million. Million is rare.

  - _AST nodes per file_: A fift of the number of bytes. Thousands to tens of thousands. Quarter million is rare.

  - _Commit size_: Half of all commits is under 10 lines. But they go up to tens of thousands of lines.
]

#make-section[What am I doing next?]

#slide[
  = CodeDiff

  - I really hate how code diffing works today. If you have ever seen a GitHub diff where somebody inlines Python code in a new function, you know what annoys me.

  - Theorethical algorithms are O(|AST|#super[2]). And some have been implemented, but in their Documentation they explicitly say they are not aiming to be robust and fast for production usage. They are mostly research artefacts.

  - The difference between:

    - Make an algoritmically better code diffing algorithm faster than O(|AST|#super[2])

    - vs

    - Make a diffing binary that can diff up to 1M AST nodes, with 47.5% of all diffs being 10 or fewer lines. The diff must be computed under 100ms for 99.99% of all commits
]

#slide[
  #show: focus

  It is easier to make a _robust_ tool if you know the real constraints.
]
#slide[
  #show: focus
  #text(size: 2.25em)[Thank you for your attention]

  #text(size: 0.9em)[#link("mailto:marko@ivankovic.me")]
]
