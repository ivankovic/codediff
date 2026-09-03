Not pure HTML. Contains templating characters that break TreeSitter HTML parsing. Because of this, requires N:M mapping for the AST, but wouldn't if it parsed correctly.
