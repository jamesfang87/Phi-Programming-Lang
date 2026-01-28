project('Phi Programming Language', ['c', 'cpp'],
  version: '0.1.0',
  default_options: [
    'cpp_std=c++26',
    'warning_level=3',
    'unity=on',           # Enable unity builds globally
    'unity_size=4'        # Adjust based on your project size
  ]
)

# Dependencies
llvm_dep = dependency('llvm',
  version: '>=18.0',
  required: true,
  method: 'config-tool',
  static: false,
  include_type: 'system'
)

json_dep = dependency('nlohmann_json', required: true)
gtest_dep = dependency('gtest', required: true, main: true)

# Include directories
inc = include_directories('include')

# Collect all source files
all_src_files = run_command(
  'find', 'src', '-name', '*.cpp',
  check: true
).stdout().strip().split('\n')

# Separate main.cpp from library sources
src_files = []
main_src = []

foreach file : all_src_files
  if file.endswith('main.cpp')
    main_src += file
  else
    src_files += file
  endif
endforeach

# Common dependencies array
common_deps = [llvm_dep, json_dep]

# Shared library for all executables (including main and tests)
# This is compiled once and reused everywhere
phi_lib = static_library('phi_lib',
  src_files,
  include_directories: inc,
  dependencies: common_deps,
  # Unity build is inherited from project defaults
)

# Main executable - just links the library
executable('phi',
  main_src,
  include_directories: inc,
  link_with: phi_lib,
  dependencies: common_deps,
  install: true
)

# Test configuration
# Common test dependencies
test_deps = common_deps + [gtest_dep]

# Test cases configuration
test_cases = {
  'lexer': ['test/unit/Lexer/LexerTest.cpp', 'test/unit/Lexer/main.cpp'],
  'parser': ['test/unit/Parser/ParserTest.cpp', 'test/unit/Parser/main.cpp'],
  'name_resolver': ['test/unit/NameResolver/NameResolverTest.cpp', 'test/unit/NameResolver/main.cpp'],
  'integration': ['test/integration/IntegrationTest.cpp', 'test/integration/main.cpp'],
}

foreach test_name, test_sources : test_cases
  test_exe = executable(test_name + '_test',
    test_sources,
    include_directories: inc,
    link_with: phi_lib,
    dependencies: test_deps,
    build_by_default: false  # Only build when running tests
  )
  
  test(test_name, test_exe)
endforeach

