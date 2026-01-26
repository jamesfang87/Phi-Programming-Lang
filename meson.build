project('Phi Programming Language', ['c', 'cpp'],
  version: '0.1.0',
  default_options: ['cpp_std=c++26', 'warning_level=3']
)

# Find LLVM using llvm-config 
llvm_dep = dependency('llvm', version: '>=18.0', required: true, 
                      method: 'config-tool', 
                      static: false,
                      include_type: 'system')
json_dep = dependency('nlohmann_json', required : true)

# Find Gtest 
gtest_dep = dependency('gtest', required: true, main: false)
gtest_main_dep = dependency('gtest', required: true, main: true)

# Include directories
inc = include_directories('include')

# Collect source files (excluding main.cpp for tests)
all_src_files = []
foreach dir : ['src']
  all_src_files += run_command('find', dir, '-name', '*.cpp', check: true).stdout().strip().split('\n')
endforeach

# Filter out main.cpp for library usage
src_files = []
main_src = []
foreach file : all_src_files
  if file.endswith('main.cpp')
    main_src += file
  else
    src_files += file
  endif
endforeach

# Main executable
executable('phi',
  all_src_files,
  include_directories: inc,
  dependencies: [llvm_dep, json_dep],
  install: true
)

# Library for tests (without main.cpp)
phi_lib = static_library('phi_lib',
  src_files,
  include_directories: inc,
  dependencies: [llvm_dep, json_dep]
)

# Test executables
test_inc = include_directories('include')

# Lexer tests
lexer_test_exe = executable('lexer_test',
  ['test/unit/Lexer/LexerTest.cpp', 'test/unit/Lexer/main.cpp'],
  include_directories: test_inc,
  link_with: phi_lib,
  dependencies: [llvm_dep, json_dep, gtest_main_dep],
)
test('lexer', lexer_test_exe)

# Parser tests
parser_test_exe = executable('parser_test',
  ['test/unit/Parser/ParserTest.cpp', 'test/unit/Parser/main.cpp'],
  include_directories: test_inc,
  link_with: phi_lib,
  dependencies: [llvm_dep, json_dep, gtest_main_dep],
)
test('parser', parser_test_exe)

# NameResolver tests
name_resolver_test_exe = executable('name_resolver_test',
  ['test/unit/NameResolver/NameResolverTest.cpp', 'test/unit/NameResolver/main.cpp'],
  include_directories: test_inc,
  link_with: phi_lib,
  dependencies: [llvm_dep, json_dep, gtest_main_dep],
)
test('name_resolver', name_resolver_test_exe)

# Integration tests
integration_test_exe = executable('integration_test',
  ['test/integration/IntegrationTest.cpp', 'test/integration/main.cpp'],
  include_directories: test_inc,
  link_with: phi_lib,
  dependencies: [llvm_dep, json_dep, gtest_main_dep],
)
test('integration', integration_test_exe)
