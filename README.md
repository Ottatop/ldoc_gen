# ℹ️ This repository has been superseded by [lcat](https://github.com/Ottatop/lcat) and is no longer being maintained.

# ldoc_gen
Generate LDoc-compatible dummy code from LuaLS annotations

## What is ldoc_gen (cooler name pending)?
`ldoc_gen` is a Rust crate meant to translate Lua code written with
[Lua language server](https://github.com/LuaLS/lua-language-server) annotations into Lua dummy code that has
[LDoc](https://github.com/lunarmodules/ldoc)-compatible ones.

This was born out of the fact that I don't really like LuaLS documentation exporting; it only exports one huge
markdown/json file, and linking references doesn't work that great. As I'm currently working on my
Lua-configurable [Wayland compositor](https://github.com/Ottatop/pinnacle), I need a better form of online
documentation, and I hope that this crate will help automate that.

## How does it work?
`ldoc_gen` uses Treesitter and regex to pick out LuaLS annotations and convert them into LDoc-compatible ones.

For example, the following Lua code:
```lua
---@param str string The input string
---@return integer ret_val The returned number
function a_function(str)
    -- body
    -- of
    -- function
end
```
is translated into:
```lua
---@tparam string str The input string
---@treturn integer The returned number
function a_function(str)

end
```

This allows you to run LDoc on the generated code to create robust documentation suitable for the web.

## How do I use it?
1. Clone the repository:
    ```sh
    git clone https://github.com/Ottatop/ldoc_gen --depth 1
    ```
2. Run `cargo`:
    ```sh
    cargo run
    ```
    This will run `ldoc_gen` on all `.lua` files in the current directory recursively, generating dummy code in
    a `.ldoc_gen` directory that you can run LDoc in.

    You can pass in two flags: `--path/-p` and `--out_dir/-o`.
    - `--path <path>` or `-p <path>`: Specify the file or directory you want generations for.
    - `--out_dir <dir>` or `-o <dir>`: Change the output directory from `./.ldoc_gen` to `<dir>/.ldoc_gen`.
3. Run `ldoc` in the generated `.ldoc_gen` directory.
    
    You may need to provide a `config.ld` file as well as a `ldoc.css` file. This will be provided in the future.

## Additional features
You can place examples in a fenced code block with a markdown header called `Example` or `Examples`.
This will generate an LDoc `@usage` annotation for you.
````lua
---A summary
---
--- ### Example
---```lua
---print("hello world")
---```
function M.thingy(thing) end

-- Will become:

---A summary
---
---@usage
---print("hello world")
function M.thingy(thing) end
````

- You can annotate something with `---@nodoc` to prevent `ldoc_gen` from generating LDoc-compatible code for it.
- Placing text in a fenced code block in the summary will translate it into four-spaced code.

## Caveats
This is *very* WIP software. The regex I wrote may not catch everything. If you encounter such a problem, please submit an issue.

Additionally, you will also need to annotate object classes with `@classmod` in LuaLS to hint to 
`ldoc_gen` that you want that class to be a class and not a module.
