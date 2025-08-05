; ModuleID = 'tests/correct/8.phi'
source_filename = "tests/correct/8.phi"
target triple = "arm64-apple-darwin24.5.0"

@0 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@1 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@2 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@3 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@4 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@5 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@6 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@7 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1

declare i32 @printf(ptr, ...)

define i64 @add(i64 %0, i64 %1) {
entry:
  %a = alloca i64, align 8
  store i64 %0, ptr %a, align 4
  %b = alloca i64, align 8
  store i64 %1, ptr %b, align 4
  %a1 = load i64, ptr %a, align 4
  %b2 = load i64, ptr %b, align 4
  %2 = add i64 %a1, %b2
  ret i64 %2
}

define i64 @sub(i64 %0, i64 %1) {
entry:
  %a = alloca i64, align 8
  store i64 %0, ptr %a, align 4
  %b = alloca i64, align 8
  store i64 %1, ptr %b, align 4
  %a1 = load i64, ptr %a, align 4
  %b2 = load i64, ptr %b, align 4
  %2 = sub i64 %a1, %b2
  ret i64 %2
}

define i64 @mul(i64 %0, i64 %1) {
entry:
  %a = alloca i64, align 8
  store i64 %0, ptr %a, align 4
  %b = alloca i64, align 8
  store i64 %1, ptr %b, align 4
  %a1 = load i64, ptr %a, align 4
  %b2 = load i64, ptr %b, align 4
  %2 = mul i64 %a1, %b2
  ret i64 %2
}

define i64 @div(i64 %0, i64 %1) {
entry:
  %a = alloca i64, align 8
  store i64 %0, ptr %a, align 4
  %b = alloca i64, align 8
  store i64 %1, ptr %b, align 4
  %a1 = load i64, ptr %a, align 4
  %b2 = load i64, ptr %b, align 4
  %2 = sdiv i64 %a1, %b2
  ret i64 %2
}

define i32 @main() {
entry:
  %a = alloca i64, align 8
  store i64 10, ptr %a, align 4
  %a1 = load i64, ptr %a, align 4
  %0 = icmp sgt i64 %a1, 0
  br i1 %0, label %if.then, label %if.else

if.then:                                          ; preds = %entry
  %1 = call i32 (ptr, ...) @printf(ptr @0, i64 1)
  br label %if.end

if.else:                                          ; preds = %entry
  %2 = call i32 (ptr, ...) @printf(ptr @1, i64 0)
  br label %if.end

if.end:                                           ; preds = %if.else, %if.then
  br label %while.cond

while.cond:                                       ; preds = %while.body, %if.end
  %a2 = load i64, ptr %a, align 4
  %3 = icmp slt i64 %a2, 20
  br i1 %3, label %while.body, label %while.end

while.body:                                       ; preds = %while.cond
  %a3 = load i64, ptr %a, align 4
  %4 = call i32 (ptr, ...) @printf(ptr @2, i64 %a3)
  %a4 = load i64, ptr %a, align 4
  %5 = add i64 %a4, 1
  store i64 %5, ptr %a, align 4
  br label %while.cond

while.end:                                        ; preds = %while.cond
  br label %for.init

for.init:                                         ; preds = %while.end
  %i = alloca i64, align 8
  store i64 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %for.init
  %loop_var = load i64, ptr %i, align 4
  %loopcond = icmp slt i64 %loop_var, 10
  %tobool = icmp ne i1 %loopcond, false
  br i1 %tobool, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %i5 = load i64, ptr %i, align 4
  %6 = call i32 (ptr, ...) @printf(ptr @3, i64 %i5)
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %inc = add i64 %loop_var, 1
  store i64 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %7 = call i64 @add(i64 1, i64 1)
  %8 = call i32 (ptr, ...) @printf(ptr @4, i64 %7)
  %9 = call i64 @sub(i64 1, i64 5)
  %10 = call i32 (ptr, ...) @printf(ptr @5, i64 %9)
  %11 = call i64 @mul(i64 6, i64 7)
  %12 = call i32 (ptr, ...) @printf(ptr @6, i64 %11)
  %13 = call i64 @div(i64 8, i64 2)
  %14 = call i32 (ptr, ...) @printf(ptr @7, i64 %13)
  ret i32 0
}
