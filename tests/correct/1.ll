; ModuleID = 'tests/correct/1.phi'
source_filename = "tests/correct/1.phi"
target triple = "arm64-apple-darwin24.5.0"

@0 = private unnamed_addr constant [12 x i8] c"Hello World\00", align 1

declare i32 @printf(ptr, ...)

define i64 @add_numbers(i64 %0, i64 %1) {
entry:
  %result = alloca i64, align 8
  %2 = add i64 %0, %1
  store i64 %2, ptr %result, align 4
  %result1 = load i64, ptr %result, align 4
  ret i64 %result1
}

define i64 @get_answer() {
entry:
  ret i64 42
}

define void @print_message() {
entry:
  %message = alloca ptr, align 8
  store ptr @0, ptr %message, align 8
  ret void
}
