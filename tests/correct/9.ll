; ModuleID = 'tests/correct/9.phi'
source_filename = "tests/correct/9.phi"
target triple = "arm64-apple-darwin24.5.0"

@0 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@1 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1
@2 = private unnamed_addr constant [6 x i8] c"%lld\0A\00", align 1

declare i32 @printf(ptr, ...)

define i32 @main() {
entry:
  %a = alloca i64, align 8
  store i64 0, ptr %a, align 4
  br label %while.cond

while.cond:                                       ; preds = %if.end, %if.then, %entry
  %a1 = load i64, ptr %a, align 4
  %0 = icmp slt i64 %a1, 10
  br i1 %0, label %while.body, label %while.exit

while.body:                                       ; preds = %while.cond
  %a2 = load i64, ptr %a, align 4
  %1 = icmp eq i64 %a2, 5
  br i1 %1, label %if.then, label %if.else

while.exit:                                       ; preds = %if.then5, %while.cond
  br label %for.init

if.then:                                          ; preds = %while.body
  %a3 = load i64, ptr %a, align 4
  %2 = add i64 %a3, 1
  store i64 %2, ptr %a, align 4
  br label %while.cond

if.else:                                          ; preds = %while.body
  %a4 = load i64, ptr %a, align 4
  %3 = icmp eq i64 %a4, 7
  br i1 %3, label %if.then5, label %if.end6

if.end:                                           ; preds = %if.end6, %after_continue
  %a7 = load i64, ptr %a, align 4
  %4 = call i32 (ptr, ...) @printf(ptr @0, i64 %a7)
  %a8 = load i64, ptr %a, align 4
  %5 = add i64 %a8, 1
  store i64 %5, ptr %a, align 4
  br label %while.cond

after_continue:                                   ; No predecessors!
  br label %if.end

if.then5:                                         ; preds = %if.else
  br label %while.exit

if.end6:                                          ; preds = %after_break, %if.else
  br label %if.end

after_break:                                      ; No predecessors!
  br label %if.end6

for.init:                                         ; preds = %while.exit
  %i = alloca i64, align 8
  store i64 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %for.init
  %loop_var = load i64, ptr %i, align 4
  %loopcond = icmp slt i64 %loop_var, 10
  %tobool = icmp ne i1 %loopcond, false
  br i1 %tobool, label %for.body, label %for.exit

for.body:                                         ; preds = %for.cond
  %i9 = load i64, ptr %i, align 4
  %6 = icmp eq i64 %i9, 5
  br i1 %6, label %if.then10, label %if.else11

for.inc:                                          ; preds = %if.end12, %if.then10
  %inc = add i64 %loop_var, 1
  store i64 %inc, ptr %i, align 4
  br label %for.cond

for.exit:                                         ; preds = %if.then15, %for.cond
  %a19 = load i64, ptr %a, align 4
  %7 = call i32 (ptr, ...) @printf(ptr @2, i64 %a19)
  ret i32 0

if.then10:                                        ; preds = %for.body
  br label %for.inc

if.else11:                                        ; preds = %for.body
  %i14 = load i64, ptr %i, align 4
  %8 = icmp eq i64 %i14, 8
  br i1 %8, label %if.then15, label %if.end16

if.end12:                                         ; preds = %if.end16, %after_continue13
  %i18 = load i64, ptr %i, align 4
  %9 = call i32 (ptr, ...) @printf(ptr @1, i64 %i18)
  br label %for.inc

after_continue13:                                 ; No predecessors!
  br label %if.end12

if.then15:                                        ; preds = %if.else11
  br label %for.exit

if.end16:                                         ; preds = %after_break17, %if.else11
  br label %if.end12

after_break17:                                    ; No predecessors!
  br label %if.end16
}
