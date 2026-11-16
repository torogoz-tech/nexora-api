# Opt-in .NET source models

These declarative models use the shared Nexora engine. No Roslyn, .NET SDK, compiler, runtime, external parser or LLM is invoked by analysis. The application being inspected is read-only. Load `csharp` explicitly with the CLI `--language-dir` option; it is not installed into the native catalog.

Current models: C#0.9 / Razor0.10 for components, plus csharp_views0.1 / razor_views0.1 for MVC views, on lexer14. The sections below retain their historical versions and measurements; current extensions are described at the end.

## C#0.3

The lexer model describes ordinary/verbatim strings, characters, comments, raw quote fences and regular/verbatim interpolations. Doubled quotes and literal braces use lexer7 complete literal escapes. Nested braces, parentheses, brackets and quoted/comment regions protect interpolation boundaries. Expression/alignment text occupies the interpolation header; `:` starts the literal format body and `}` terminates a format-free header or the formatted region. These are source spans, not decoded or evaluated values. BOM trivia preserves original offsets; C# comment line terminators are explicitly modeled.

The grammar extracts file/block namespaces, class/interface/enum declarations and record classes. Attributes, modifiers, generic/base/constraint/primary-constructor headers and member bodies remain opaque. Struct/record-struct shapes retain their AST kinds and named Statement graph nodes; no normalized struct declaration kind or member/type binding is claimed. Records are not expanded into synthesized constructors/properties/equality members. Imports, methods, locals, types, overloads, partial declarations across files and call targets are not resolved.

An unresolved preprocessor conditional shields the rest of the file as opaque. The engine does not evaluate build symbols or treat inactive branches as declarations. This conservatively also leaves declarations after the condition unknown. Raw interpolated strings with variable dollar/brace arity are explicitly rejected pending a shared delimiter capability. Raw multiline indentation and quote-run legality, valid escape spellings/character lengths, decoded identifiers, UTF8 suffix semantics and full language conformance remain unvalidated. Ordinary literal guards catch unescaped newlines; prefix escapes still lack per-language validation of their following scalar.

Run the public `fixtures/csharp/Sample.cs` with `nexora --language-dir templates/dotnet-understanding/csharp analyze <absolute-path-to-Sample.cs>`. Its class is extracted while its members remain opaque. `complete` is not implied by successful graph construction. No `.NET` build is needed or performed.

## Razor0.2

The profile now includes bounded implicit expressions, member/call/index continuations and contextual nullable operators, alongside explicit regions. Load the full catalog and select the host: `nexora --language-dir templates/dotnet-understanding --language razor analyze <absolute-path-to-Sample.razor>`. See [boundary contract and calibration](../../docs/semantic-understanding/razor-delimited-foundation.md).

C#0.3 adds opaque entry contexts for member/body/expression fragments without changing its root declaration grammar. Razor routes only delimited source ranges into those contexts, with isolated scopes and no shared-component bindings. Nested attributes, explicit expressions, Razor comments and doubled-at headers have public regressions. Provider C# interpretation of a Razor comment can still be Partial; its host boundary is protected.

This is a partial calibration, not full Razor support: 191 private files produce graphs, 312 C# units are isolated (309 opaque, 3 partial), and 112 unknown tails still leave substantial source uninterpreted. The earlier delimited baseline isolated21units with150unknown tails. Await/control forms, full component-attribute semantics, markup-in-code disambiguation and shared semantic scopes remain pending. Implicit forms beyond the tested continuation grammar also remain open. Script/style content is not yet interpreted as JavaScript/CSS. Unknown tails conservatively hide later legitimate regions, so successful graph construction must not be presented as completed understanding.

See [non-consuming boundaries and implicit limits](../../docs/semantic-understanding/nonconsuming-boundaries.md). Untyped quoted/unquoted component attributes remain opaque. Email-like text follows Razor UTF16char classification. Unsupported `@<...>` in code preserves prefix graphs and marks the enclosing interpretation Partial; raw interpolation arity remains explicitly rejected.

## Razor0.3 / C#0.4: mixed control regions

The current profile adds parenthesized control headers and mixed statement/markup bodies using lexer10 mode-only token rules. C#0.4 adds only the opaque `control_header_opaque` entry context; root declaration behavior is unchanged. These versions supersede the profile versions described in the historical measurements above. New measurements must use their own model fingerprints.

Recognized control boundaries, nested markup, literal braces, void/self-closing tags and C# generic/comparison statement envelopes have public regressions. Whole statement envelopes remain guards, and headers remain opaque C# units. This does not establish complete control-flow semantics, bindings or call resolution. Else/try/do/await, advanced statements, shared component scopes and full `.cshtml` conformance remain pending. See [contract and limits](../../docs/semantic-understanding/razor-control-regions.md).

## Razor0.4: contextual continuations

Lexer11 preserves root trivia regions without replacing significant context. Razor0.4 separates conditional kinds from other controls, then uses contextual rules for else-if/else in root, nested code, elements, HTML comments and markup lines. Public regressions prevent extraction from C# strings, emails and literal escaped-at gaps. C#0.4 remains unchanged.

This adds lexical boundaries and opaque condition ranges, not a control-flow graph or deep C# semantics. Ambiguous comments after a conditional remain guarded; unsupported separators containing comments and unsupported line-carrier continuations remain unknown. Broader dialect/template/member/type/binding work is still pending. See [contract, evidence and limits](../../docs/semantic-understanding/trivia-regions-and-branches.md).

## Razor0.5: inline template host boundaries

Lexer12 shares transition pattern compilation with bounded distinct definitions and phase references. Razor0.5 reuses captured element/void/raw modes for `@<...>` inside code, protecting delimiters and keeping following source available. Nested expressions retain original ranges; C# strings that contain template-looking text remain literal.

The pure C#0.4 provider is unchanged and can still report a partial member unit when it contains Razor syntax. The host's recovered boundary is separate evidence from that provider failure. Default diagnostic files remain uncached; the structural-view cache test is explicit and does not alter production configuration. See [contract and evidence](../../docs/semantic-understanding/shared-transitions-and-inline-templates.md) and `fixtures/razor/InlineTemplatesSample.razor`.


## Razor0.6 / C#0.5: explicit lexical dialect entry

Razor routes select `lexical_context: razor` (composition6). C#lexer13 declares that entry using local rules and the copied mixed modes, while its normal root retains the pure modes and rejects top-level Razor transitions. Interpolated-string categories are distinct for the selected dialect. Both graph molds use schema10; generic runtime/parser code is unchanged.

Member/body/expression/header grammars remain opaque and scopes isolated. Correct delimitation is not deep C# semantic understanding. Public tests verify nested expressions are not duplicated, template-looking literal strings remain literal, and production cache reuse is invalidated by provider-rule changes. Unsupported constructs retain conservative unknown/partial coverage. Historical measurements above retain their original versions and hashes; use the new contextual-dialect report for fresh evidence.

## Razor0.7 / C#0.6: mixed code blocks inside carriers

`@{...}` now has a declared code_block context inside element, raw, HTML-comment and line carriers. Its body uses statement/markup phase rules and selects the provider entry `razor_body`; ordinary expressions/members keep `razor`, and the normal C# root keeps its pure rules. This is data specialization with lexer13/composition6/graph10, without new Rust runtime branches.

Protected directive lines prevent the statement wrapper from absorbing `#endif` and the block closer. They do not implement preprocessing or prove branch semantics. Bodies and expressions remain opaque, with isolated scopes. See [contract, evidence and remaining recovery work](../../docs/semantic-understanding/embedded-code-blocks.md).

## Razor0.8 / C#0.7: local statement recovery

`csharp_statement.recovery_prefixes: ['}']` uses the existing ancestor-closing recovery mechanism. The statement remains unclosed/Partial; its parent consumes the real closer, preserving neighboring code. Strings, comments, groups and recovery barriers retain ownership. EOF cannot fabricate a closer. The Rust runtime/parser remain unchanged.

Recovered outer boundaries do not imply that the inner sentence is valid or understood. Diagnostic files remain retryable rather than cached as successes. See [range, graph, cache evidence and limits](../../docs/semantic-understanding/local-statement-recovery.md).

## C#0.8 / Razor0.9: counted raw interpolation

Lexer14 derives a child's delimiter count from an immediate-parent opening capture, using bounded literal transitions. The raw parent retains its captured quote fence; the expression/format child receives counted braces and a counted header terminal. Nested strings, comments, groups and raw strings retain their own boundaries. C# selects a separate local rule only in the declared Razor lexical entries; its pure root does not gain Razor syntax.

Public tests cover two generic syntaxes, UTF8, capture/placement cache invalidation, nested C#/Razor, literal text without false declarations, original ranges, six line endings and explicit incomplete regions. Headers/formats remain guards with opaque semantics; no types, calls, decoded values or full C# indentation/conformance are implied. See [contract and evidence](../../docs/semantic-understanding/counted-lexical-transitions.md). Private reports from earlier engines keep their original attribution.

## C#0.9 / Razor0.10: body expression contexts and file kinds

The body phase recognizes `@identifier` as its own expression input. Bare unterminated C# statements stay Partial; escaped identifiers inside existing statements and literals do not create Razor units. Known reserved forms remain guarded, and their boundaries follow the declared Unicode identifier-part set. All changes are data-only on engine0.33.

`.razor` selects razor/csharp; `.cshtml` selects razor_views/csharp_views. MVC-specific directive names such as model remain available as ordinary component expression names. The view pair preserves its own directive guards and provider routes; csview is a synthetic fragment-provider extension. Load the catalog and select razor_views explicitly for view-only CLI runs. Normal .cs remains the pure csharp root. [Context contract, references, tests and limits](../../docs/semantic-understanding/razor-body-expressions.md).
