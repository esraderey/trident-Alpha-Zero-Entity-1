/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

module.exports = grammar({
  name: "trident",

  extras: ($) => [/\s/, $.line_comment],

  word: ($) => $.identifier,

  conflicts: ($) => [],

  rules: {
    source_file: ($) =>
      seq(
        $._file_header,
        repeat($.use_declaration),
        repeat($.io_declaration),
        repeat($._item),
      ),

    // ---- File header ----

    _file_header: ($) =>
      choice($.program_declaration, $.module_declaration),

    program_declaration: ($) =>
      seq("program", field("name", $.identifier)),

    module_declaration: ($) =>
      seq("module", field("name", $.module_path)),

    // ---- Use declarations ----

    use_declaration: ($) => seq("use", $.module_path),

    module_path: ($) =>
      prec.left(
        seq($.identifier, repeat(seq(".", $.identifier))),
      ),

    // ---- I/O declarations ----

    io_declaration: ($) =>
      choice(
        $.pub_io_declaration,
        $.sec_io_declaration,
        $.sec_ram_declaration,
      ),

    pub_io_declaration: ($) =>
      seq(
        "pub",
        field("kind", alias($.identifier, $.io_kind)),
        ":",
        field("type", $._type),
      ),

    sec_io_declaration: ($) =>
      prec(
        1,
        seq(
          "sec",
          field("kind", alias($.identifier, $.io_kind)),
          ":",
          field("type", $._type),
        ),
      ),

    sec_ram_declaration: ($) =>
      prec(
        2,
        seq(
          "sec",
          "ram",
          ":",
          "{",
          optional(commaSep1($.ram_entry)),
          optional(","),
          "}",
        ),
      ),

    ram_entry: ($) =>
      seq(
        field("address", $.integer_literal),
        ":",
        field("type", $._type),
      ),

    // ---- Items ----

    _item: ($) =>
      choice(
        $.const_definition,
        $.struct_definition,
        $.event_definition,
        $.function_definition,
      ),

    const_definition: ($) =>
      seq(
        optional("pub"),
        "const",
        field("name", $.identifier),
        ":",
        field("type", $._type),
        "=",
        field("value", $._expression),
      ),

    struct_definition: ($) =>
      seq(
        optional("pub"),
        "struct",
        field("name", $.identifier),
        "{",
        optional(commaSep1($.struct_field)),
        optional(","),
        "}",
      ),

    struct_field: ($) =>
      seq(
        optional("pub"),
        field("name", $.identifier),
        ":",
        field("type", $._type),
      ),

    event_definition: ($) =>
      seq(
        "event",
        field("name", $.identifier),
        "{",
        optional(commaSep1($.event_field)),
        optional(","),
        "}",
      ),

    event_field: ($) =>
      seq(
        field("name", $.identifier),
        ":",
        field("type", $._type),
      ),

    function_definition: ($) =>
      seq(
        optional("pub"),
        optional($.attribute),
        "fn",
        field("name", $.identifier),
        "(",
        optional(commaSep1($.parameter)),
        optional(","),
        ")",
        optional(seq("->", field("return_type", $._type))),
        optional(field("body", $.block)),
      ),

    attribute: ($) =>
      seq(
        "#",
        "[",
        $.identifier,
        "(",
        $.identifier,
        ")",
        "]",
      ),

    parameter: ($) =>
      seq(
        field("name", $.identifier),
        ":",
        field("type", $._type),
      ),

    // ---- Types ----

    _type: ($) =>
      choice(
        $.primitive_type,
        $.array_type,
        $.tuple_type,
        $.named_type,
      ),

    primitive_type: ($) =>
      choice("Field", "XField", "Bool", "U32", "Digest"),

    array_type: ($) =>
      seq(
        "[",
        field("element", $._type),
        ";",
        field("size", $.integer_literal),
        "]",
      ),

    tuple_type: ($) =>
      seq(
        "(",
        $._type,
        repeat1(seq(",", $._type)),
        optional(","),
        ")",
      ),

    named_type: ($) => $.module_path,

    // ---- Block ----

    block: ($) =>
      seq(
        "{",
        repeat($._statement),
        optional(field("tail", $._expression)),
        "}",
      ),

    // ---- Statements ----

    _statement: ($) =>
      choice(
        $.let_statement,
        $.if_statement,
        $.for_statement,
        $.return_statement,
        $.match_statement,
        $.asm_block,
        $.reveal_statement,
        $.seal_statement,
        $.assignment_statement,
        $.expression_statement,
      ),

    let_statement: ($) =>
      seq(
        "let",
        optional("mut"),
        field("pattern", $._pattern),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expression),
      ),

    _pattern: ($) =>
      choice($.identifier, $.tuple_pattern, "_"),

    tuple_pattern: ($) =>
      seq(
        "(",
        commaSep1(choice($.identifier, "_")),
        optional(","),
        ")",
      ),

    assignment_statement: ($) =>
      prec(
        -1,
        seq(
          field("place", $._expression),
          "=",
          field("value", $._expression),
        ),
      ),

    if_statement: ($) =>
      seq(
        "if",
        field("condition", $._expression),
        field("then", $.block),
        optional(
          seq(
            "else",
            field("else", choice($.if_statement, $.block)),
          ),
        ),
      ),

    for_statement: ($) =>
      seq(
        "for",
        field("variable", choice($.identifier, "_")),
        "in",
        field("start", $._expression),
        "..",
        field("end", $._expression),
        optional(
          seq("bounded", field("bound", $.integer_literal)),
        ),
        field("body", $.block),
      ),

    return_statement: ($) =>
      prec.left(seq("return", optional($._expression))),

    match_statement: ($) =>
      seq(
        "match",
        field("scrutinee", $._path_expr),
        "{",
        optional(commaSep1($.match_arm)),
        optional(","),
        "}",
      ),

    match_arm: ($) =>
      seq(
        field("pattern", $.match_pattern),
        "=>",
        field("body", $.block),
      ),

    match_pattern: ($) =>
      choice($.integer_literal, $.boolean_literal, "_"),

    asm_block: ($) =>
      prec(
        20,
        seq(
          "asm",
          optional($.asm_annotation),
          "{",
          optional($.asm_body),
          "}",
        ),
      ),

    asm_annotation: ($) =>
      seq(
        "(",
        choice(
          // target and effect: asm(triton, +3)
          seq(
            field("target", $.identifier),
            ",",
            field("effect", $.asm_effect),
          ),
          // target only: asm(triton)
          field("target", $.identifier),
          // effect only: asm(+3) or asm(-2)
          field("effect", $.asm_effect),
        ),
        ")",
      ),

    asm_effect: ($) => /[+-][0-9]+/,

    asm_body: ($) => repeat1($.asm_instruction),

    asm_instruction: ($) =>
      choice(
        $.identifier,
        $.integer_literal,
        $.line_comment,
      ),

    reveal_statement: ($) =>
      seq(
        "reveal",
        field("event", $.identifier),
        "{",
        optional(commaSep1($.field_init)),
        optional(","),
        "}",
      ),

    seal_statement: ($) =>
      seq(
        "seal",
        field("event", $.identifier),
        "{",
        optional(commaSep1($.field_init)),
        optional(","),
        "}",
      ),

    field_init: ($) =>
      seq(
        field("name", $.identifier),
        optional(seq(":", field("value", $._expression))),
      ),

    expression_statement: ($) => prec(-2, $._expression),

    // ---- Expressions ----

    _expression: ($) =>
      choice(
        $.integer_literal,
        $.boolean_literal,
        $._path_expr,
        $.binary_expression,
        $.call_expression,
        $.index_expression,
        $.struct_init_expression,
        $.array_init_expression,
        $.tuple_expression,
        $.parenthesized_expression,
      ),

    integer_literal: ($) => /[0-9]+/,

    boolean_literal: ($) => choice("true", "false"),

    // Identifier or dotted path (also handles field access via parsing as dotted path)
    _path_expr: ($) => $.module_path,

    // Binary operators — precedence matches parser.rs op_binding_power
    binary_expression: ($) =>
      choice(
        prec.left(
          12,
          seq(
            field("left", $._expression),
            "/%",
            field("right", $._expression),
          ),
        ),
        prec.left(
          10,
          seq(
            field("left", $._expression),
            "&",
            field("right", $._expression),
          ),
        ),
        prec.left(
          10,
          seq(
            field("left", $._expression),
            "^",
            field("right", $._expression),
          ),
        ),
        prec.left(
          8,
          seq(
            field("left", $._expression),
            "*",
            field("right", $._expression),
          ),
        ),
        prec.left(
          8,
          seq(
            field("left", $._expression),
            "*.",
            field("right", $._expression),
          ),
        ),
        prec.left(
          6,
          seq(
            field("left", $._expression),
            "+",
            field("right", $._expression),
          ),
        ),
        prec.left(
          4,
          seq(
            field("left", $._expression),
            "<",
            field("right", $._expression),
          ),
        ),
        prec.left(
          2,
          seq(
            field("left", $._expression),
            "==",
            field("right", $._expression),
          ),
        ),
      ),

    call_expression: ($) =>
      prec(
        15,
        seq(
          field("function", $.module_path),
          "(",
          optional(commaSep1($._expression)),
          optional(","),
          ")",
        ),
      ),

    index_expression: ($) =>
      prec.left(
        16,
        seq(
          field("object", $._expression),
          "[",
          field("index", $._expression),
          "]",
        ),
      ),

    struct_init_expression: ($) =>
      prec(
        1,
        seq(
          field("name", $.module_path),
          "{",
          optional(commaSep1($.field_init)),
          optional(","),
          "}",
        ),
      ),

    array_init_expression: ($) =>
      seq(
        "[",
        optional(
          seq(commaSep1($._expression), optional(",")),
        ),
        "]",
      ),

    tuple_expression: ($) =>
      seq(
        "(",
        $._expression,
        ",",
        optional(
          seq(commaSep1($._expression), optional(",")),
        ),
        ")",
      ),

    parenthesized_expression: ($) =>
      seq("(", $._expression, ")"),

    // ---- Terminals ----

    identifier: ($) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    line_comment: ($) => token(seq("//", /.*/)),
  },
});

/**
 * Comma-separated list with at least one element.
 * @param {Rule} rule
 */
function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)));
}
