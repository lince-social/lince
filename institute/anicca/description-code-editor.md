# Code inside descriptions

Plain Records can contain code fences and heredocs. Command Records use Bash for the whole description. Both use the same editor support.

Support Nix, TOML, Lingua, Scheme and Bash. Choose the Scheme dialect before adding its language server.

Read the language from a code fence. For a heredoc, use the destination file extension when the path is clear. Allow a language override when it is not. Recognize a bare heredoc as a bounded Bash section without treating surrounding prose as shell code.

Keep track of nested sections, such as a Nix file inside a Bash heredoc inside a description. Use the innermost section for highlighting, completion, hover, errors and formatting.

Give language servers virtual documents for each section. Map their results back to the original description. Keep project paths available for imports and file references. Reject edits from old document versions and edits outside the section. Share servers by language and project instead of starting one for every snippet.

Reuse Lingua's parser and formatter. Add its completion, hover and error reporting. Highlighting, formatting and language-server support are separate capabilities; show when a required tool is unavailable.

Formatting is an explicit action with undo. Preserve the surrounding prose, code fences and heredoc markers. Format inner sections with their own formatter and keep their contents intact when formatting the outer Bash script. Never format a command silently when running it.

A quoted opening marker, such as <<'EOF', makes the heredoc literal. Its closing marker is bare EOF on its own line. Unquoted markers allow Bash expansion. Preserve this difference and handle tab-stripping heredocs without changing what the script writes.

Check nested sections, incomplete input, Unicode positions, stale server replies, undo, unchanged surrounding text and large descriptions. Opening or editing a description never runs its code. Include licenses and credits for embedded dependencies in the Sand.
