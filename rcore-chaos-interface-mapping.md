# rCore 被合并模块接口文档 & Chaos 实现对照

本文档记录 rCore 中被 chaos 合并的模块（sync、process、fs、signal、ipc、memory、trap）的公共接口，以及 chaos `kernel.rs` 中的对应实现。所有源码位置均标注了具体文件和行号。

> **rCore 源码**: `/home/user/src/rCore/kernel/src/`
> **Chaos 源码**: `/home/user/src/chaos/kernel/src/kernel.rs`（6,386 行）

---

## 1. 同步原语（sync → kernel.rs L210-640）

### 1.1 互斥锁

#### rCore 接口

| 接口 | 位置 | 说明 |
|---|---|---|
| `Mutex<T, S>` | `sync/mutex.rs:41` | 泛型互斥锁，S 为 MutexSupport |
| `SpinLock<T>` | `sync/mutex.rs:37` | `type SpinLock<T> = Mutex<T, Spin>` |
| `SpinNoIrqLock<T>` | `sync/mutex.rs:38` | `type SpinNoIrqLock<T> = Mutex<T, SpinNoIrq>` |
| `SleepLock<T>` | `sync/mutex.rs:39` | `type SleepLock<T> = Mutex<T, Condvar>` |
| `MutexGuard<'a, T, S>` | `sync/mutex.rs:52` | RAII 锁守卫 |
| `trait MutexSupport` | `sync/mutex.rs:247` | 锁策略 trait（new/cpu_relax/before_lock/after_unlock） |
| `struct Spin` | `sync/mutex.rs:260` | 纯自旋策略 |
| `struct SpinNoIrq` | `sync/mutex.rs:277` | 自旋+禁中断策略 |
| `struct FlagsGuard` | `sync/mutex.rs:280` | 中断标志守卫 |

**核心方法（`impl Mutex`）**:
- `new(user_data: T) -> Self` (L79)
- `lock(&self) -> MutexGuard<T, S>` (L136)
- `busy_lock(&self) -> MutexGuard<T, S>` (L149)
- `try_lock(&self) -> Option<MutexGuard<T, S>>` (L193)
- `force_unlock(&self)` (L187)
- `into_inner(self) -> T` (L90)

#### Chaos 实现

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct Spin` | `kernel.rs:267` | 简单 AtomicBool 自旋锁 |
| `struct KernLock` | `kernel.rs:210` | 全局内核锁，用 `Mutex<Vec<usize>>` 跟踪持有者栈 |
| `static GKL` | `kernel.rs:244` | 全局 KernLock 实例 |

**Chaos `Spin` 方法**:
- `new() -> Self` (L266)
- `acquire(&self)` (L267) — CAS 自旋
- `try_acquire(&self) -> bool` (L272)
- `release(&self)` (L275) — AtomicBool store
- `is_held(&self) -> bool` (L276)

**Chaos `KernLock` 方法**:
- `new() -> Self` (L212)
- `enter(&self, id: usize)` (L215) — 按 ID 递增顺序入栈
- `leave(&self)` (L225) — pop 栈顶
- `held() -> bool` (L229)
- `owner() -> usize` (L230)
- `level() -> usize` (L231)
- `try_enter(&self, id: usize) -> bool` (L232)

**关键差异**: rCore 用泛型 `Mutex<T, S>` + RAII Guard；Chaos 用简单 `Spin`（AtomicBool）+ `KernLock`（Mutex<Vec>），无 RAII Guard，无禁中断支持。`FlgGuard`（L284）是空操作占位。

---

### 1.2 条件变量

#### rCore 接口

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Condvar` | `sync/condvar.rs:17` | 等待队列 + epoll 队列 |
| `struct RegisteredProcess` | `sync/condvar.rs:9` | epoll 注册信息 |

**核心方法**:
- `new() -> Self` (L23)
- `wait<'a, T, S>(&self, guard: MutexGuard) -> MutexGuard` (L90)
- `wait_timeout(&self, guard, timeout: TimeSpec) -> Option<MutexGuard>` (L109)
- `wait_event<T>(condvar, condition: impl FnMut() -> Option<T>) -> T` (L50)
- `wait_events<T>(condvars, condition) -> T` (L55)
- `notify_one(&self)` (L139)
- `notify_all(&self)` (L149)
- `notify_n(&self, n: usize) -> usize` (L160)
- `register_epoll_list(&self, proc, tid, epfd, fd)` (L177)
- `unregister_epoll_list(&self, tid, epfd, fd) -> bool` (L192)

#### Chaos 实现

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct SyncQueue` | `kernel.rs:358` | 条件变量替代品 |

**SyncQueue 方法**:
- `new() -> Self` (L364)
- `park_on<T>(&self, g: &Mutex<T>, pred: impl Fn(&T) -> bool) -> bool` (L374) — 带谓词的等待
- `signal(&self)` (L393) — 唤醒一个
- `broadcast(&self)` (L401) — 唤醒全部
- `signal_n(&self, n: usize) -> usize` (L407) — 唤醒 n 个
- `pending(&self) -> usize` (L420)
- `wait_ev<T>(&self, g, cond) -> bool` (L421)
- `wait_events<T>(queues, g, cond) -> bool` (L428) — 多队列等待
- `wait_guard<T>(&self, g)` (L441)
- `wait_timeout<T>(&self, g, timeout: Duration) -> bool` (L446)
- `reg_epoll(&self, task_id, epfd, fd)` (L452)
- `unreg_epoll(&self, task_id, epfd, fd) -> bool` (L455)

**关键差异**: rCore 的 Condvar 返回 MutexGuard（可重新获取锁）；Chaos 的 SyncQueue 在 park 前释放锁，唤醒后不自动重获锁。`park_on` 接受谓词函数，被唤醒后返回 `false`（标记 `// HUMAN`）。

---

### 1.3 信号量

#### rCore 接口

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Semaphore` | `sync/semaphore.rs:16` | 异步信号量 |
| `struct SemaphoreGuard<'a>` | `sync/semaphore.rs:30` | RAII 访问守卫 |

**核心方法**:
- `new(count: isize) -> Self` (L40)
- `async acquire(&self) -> Result<(), SysError>` (L62)
- `release(&self)` (L105)
- `async access(&self) -> Result<SemaphoreGuard<'_>, SysError>` (L118)
- `remove(&self)` (L51)
- `get/isize` (L124), `get_ncnt/usize` (L128), `get_pid/usize` (L132), `set_pid(usize)` (L136), `set(isize)` (L141)

#### Chaos 实现

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct Sema` | `kernel.rs:469` | 阻塞式信号量 |
| `struct SemaGuard<'a>` | `kernel.rs:471` | RAII 守卫 |

**Sema 方法**:
- `new(c: isize) -> Self` (L474)
- `remove(&self)` (L477)
- `release(&self)` (L482)
- `try_acquire(&self) -> Result<bool, &str>` (L487)
- `acquire_spin(&self) -> Result<(), &str>` (L498) — 忙等
- `access(&self) -> Result<SemaGuard, &str>` (L506)
- `get_val() -> isize` (L510), `get_ncnt() -> usize` (L511)
- `get_pid() -> usize` (L512), `set_pid(usize)` (L513), `set_val(isize)` (L514)

**关键差异**: rCore 用 async/await；Chaos 用阻塞式 `acquire_spin()`（忙等）和 `try_acquire()`。内部结构 `SemaInner`（L467）用 EvBus 替代 EventBus。

---

### 1.4 事件总线

#### rCore 接口

| 接口 | 位置 | 说明 |
|---|---|---|
| `bitflags Event` | `sync/event_bus.rs:13` | READABLE/WRITABLE/ERROR/CLOSED/PROCESS_QUIT 等 |
| `type EventHandler` | `sync/event_bus.rs:31` | `Box<dyn Fn(Event) -> bool + Send>` |
| `struct EventBus` | `sync/event_bus.rs:34` | event + callbacks |
| `fn wait_for_event` | `sync/event_bus.rs:72` | async 等待事件 |

**EventBus 方法**: `new() -> Arc<Mutex<Self>>`, `set(Event)`, `clear(Event)`, `change(reset, set)`, `subscribe(EventHandler)`, `get_callback_len()`

#### Chaos 实现

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct EvBus` | `kernel.rs:304` | `ev: u32` + `cbs: Vec<Box<dyn Fn(u32)->bool+Send>>` |
| `struct EvFlag` | `kernel.rs:288` | 常量集（READABLE=1<<0, WRITABLE=1<<1, ...） |
| `fn wait_ev` | `kernel.rs:321` | 阻塞等待 |

**EvBus 方法**: `make() -> Arc<Mutex<Self>>` (L309), `set(u32)` (L310), `clear(u32)` (L311), `change(rst, set)` (L312), `sub(cb)` (L317), `cb_len()` (L318)

**关键差异**: rCore 用 bitflags `Event`；Chaos 用裸 `u32` + `EvFlag` 常量。rCore 有 async `wait_for_event`；Chaos 用阻塞循环 `wait_ev`。

---

### 1.5 Futex

#### rCore 接口

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Futex` | `process/futex.rs:23` | 单一 futex 实现 |
| `struct Waiter` | `process/futex.rs:13` | async 等待者 |

**方法**: `new()` (L28), `wake(count: usize) -> usize` (L36), `async wait(timeout: Option<Duration>) -> SysResult` (L52)

#### Chaos 实现

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct FutexBucket` | `kernel.rs:527` | 支持超时的 futex |
| `struct FutexTable` | `kernel.rs:576` | 简化版 futex |

**FutexBucket 方法**: `new()` (L531), `wait(addr, expected, val, timeout) -> Result` (L532), `wake(addr, count) -> usize` (L540), `requeue(src, dst, wake_n, move_n) -> usize` (L553), `pending_at(addr) -> usize` (L571)

**FutexTable 方法**: `new()` (L581), `ftx_wait(addr, expected, val) -> bool` (L583), `ftx_wake(addr, count) -> usize` (L592), `ftx_requeue(src, dst, wake_n, move_n) -> usize` (L615)

**关键差异**: rCore 用 Arc<Futex> + async Waker；Chaos 提供两套实现——FutexBucket（超时+地址匹配）和 FutexTable（简化版），均为阻塞式。

---

## 2. 进程管理（process → kernel.rs L4111-4575）

### 2.1 进程标识

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Pid(pub usize)` | `process/proc.rs:44` | 进程 ID 包装 |
| `Pid::INIT = 1` | `process/proc.rs:48` | init 进程常量 |
| `Pid::new() -> Self` | `process/proc.rs:50` | 分配新 PID |
| `fn process_of(tid) -> Option<Arc<Mutex<Process>>>` | `process/proc.rs:134` | 通过 TID 查进程 |
| `fn process(pid) -> Option<Arc<Mutex<Process>>>` | `process/proc.rs:143` | 通过 PID 查进程 |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct Pid(pub usize)` | `kernel.rs:4111` | 同 rCore |
| `Pid::new()` | `kernel.rs:4112` | 自增分配 |
| `struct TaskTable` | `kernel.rs:4393` | 全局进程表 |

**TaskTable 方法**: `spawn()` (L4402), `spawn_root()` (L4408), `find(id) -> Option<Arc<Task>>` (L4413), `find_by_tag(tag) -> Vec<Arc<Task>>` (L4416), `process_of_tid(tid) -> Option<Arc<Task>>` (L4419), `pgid_group(pgid) -> Vec<Arc<Task>>` (L4424), `register(pid, task)` (L4429), `reap(tid) -> Option<Arc<Task>>` (L4433), `fork_task(src) -> Arc<Task>` (L4449)

---

### 2.2 进程结构

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Process` | `process/proc.rs:73` | 进程主体，含 vm/files/cwd/threads/eventbus/signal 等 |
| `fn get_free_fd(&self) -> usize` | L168 | 获取空闲 fd |
| `fn add_file(&mut self, file_like) -> usize` | L180 | 添加文件 |
| `fn get_futex(&mut self, uaddr) -> Arc<Futex>` | L187 | 获取 futex |
| `fn exit(&mut self, exit_code: usize)` | L196 | 进程退出 |
| `fn exited(&self) -> bool` | L225 | 是否已退出 |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct Task` | `kernel.rs:4141` | 进程+线程合并体 |
| `struct TaskInfo` | `kernel.rs:4123` | 进程元信息 |

**Task 方法**:
- `make(tag) -> Arc<Self>` (L4165) — 创建新任务
- `add_file(&self, fl: FLike) -> usize` (L4204)
- `get_file(&self, fd) -> Option<FLike>` (L4209)
- `exit_proc(&self, code: usize)` (L4214) — 关闭所有 fd + 发事件
- `exited(&self) -> bool` (L4258)
- `begin_run(&self) -> ThdCtx` (L4277) — 取出线程上下文
- `end_run(&self, cx: ThdCtx)` (L4291) — 存回线程上下文
- `has_sig(&self) -> bool` (L4300)
- `send_sig(&self, signo, sender_tid)` (L4316)
- `close_fd(&self, fd) -> Result` (L4327)
- `dup_fd(&self, old_fd, cloexec) -> Result<usize>` (L4334)
- `dup2_fd(&self, old_fd, new_fd) -> Result<usize>` (L4350)
- `fd_count(&self) -> usize` (L4363)
- `set_cloexec(&self, fd, val) -> Result` (L4370)

**关键差异**: rCore 分离 Process + Thread；Chaos 合并为 Task，每个字段都用独立 Mutex 保护（而非整个 Process 用一把大锁）。

---

### 2.3 线程

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Thread` | `process/thread.rs:79` | inner + vm + proc + tid |
| `struct ThreadContext` | `process/thread.rs:57` | user context + fp state |
| `fn new_user(inode, path, args, envs) -> Arc<Thread>` | L231 | 创建用户线程 |
| `fn fork(&self, tf: &UserContext) -> Arc<Thread>` | L365 | fork |
- `fn begin_running(&self) -> ThreadContext` (L468)
- `fn end_running(&self, cx: ThreadContext)` (L472)
- `fn has_signal_to_handle(&self) -> bool` (L477)
- `pub fn spawn(thread: Arc<Thread>)` (L496) — 异步 spawn

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct ThdCtx` | `kernel.rs:4130` | uctx + clear_tid + smask |
| `struct Context` | `kernel.rs:3572` | r:[u64;N_REGS] + ip + flags |

**Context 方法**: `new()` (L3578), `capture(r, ip, flags)` (L3579)

**ThdCtx 方法**: `default()` (L4135)

**关键差异**: rCore Thread 有完整的新建/fork/clone 能力；Chaos 中线程操作由 TaskTable 统一管理（`fork_task` L4449, `clone_thread` L4493）。

---

## 3. 信号处理（signal → kernel.rs L189-3484）

### 3.1 信号定义

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `enum Signal` | `signal/mod.rs:16` | 64 个 POSIX 信号（SIGHUP=1 到 SIGRT64=64） |
| `Signal::is_standard(&self) -> bool` | L89 |
| `fn send_signal(process, tid, info)` | L95 |
| `fn handle_signal(thread, tf) -> bool` | L132 |

#### Chaos

Chaos 没有完整的 Signal enum。信号用 `i32` 数字直接表示，通过常量引用（如 `SIGKILL`, `SIGSTOP`）。

**SigSet 方法** (kernel.rs:3395-3484):

- `new() -> Self` (L3396) — 初始化 NSIG 个默认 action
- `sig_pending(sig: u32) -> bool` (L3404)
- `sig_raise(sig: u32)` (L3408) — 置位 pending
- `coalesce_pending() -> u64` (L3414) — 返回 pending & !blocked
- `sig_clear(sig: u32)` (L3425)
- `sig_block(sig: u32)` (L3431), `sig_unblock(sig: u32)` (L3436), `sig_setmask(mask: u64)` (L3440)
- `deliverable() -> Option<u32>` (L3444) — 返回最高优先级可投递信号
- `set_action(signo, action)` (L3455), `get_action(signo) -> &SigAction` (L3461)
- `is_ignored(signo) -> bool` (L3469)
- `clear_non_caught()` (L3477)

---

### 3.2 信号动作

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Sigset(u64)` | `signal/action.rs:25` | 位图信号集 |
| `struct SignalAction` | `signal/action.rs:54` | handler + flags + restorer + mask |
| `struct Siginfo` | `signal/action.rs:94` | signo + errno + code + field |
| `struct SignalStack` | `signal/action.rs:282` | sp + flags + size |
| `bitflags SignalActionFlags` | `signal/action.rs:101` | SA_ONSTACK/SA_RESTART/SA_NODEFER 等 |
| 常量 `SIG_DFL/SIG_IGN/SIG_ERR` | `signal/action.rs:6-8` |
| 常量 `SI_USER/SI_KERNEL/...` | `signal/action.rs:10-19` |

**Sigset 方法**: `empty()`, `contains(sig)`, `add(sig)`, `add_set`, `remove(sig)`, `remove_set`

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct SigAction` | `kernel.rs:189` | handler: usize + flags: u32 + mask: u64 |
| `struct SigSet` | `kernel.rs:195` | pending: u64 + blocked: u64 + actions: Vec\<SigAction\> |

**关键差异**: rCore 用独立 `Sigset(u64)` + `SignalAction`；Chaos 将 pending/blocked/actions 合入一个 `SigSet` 结构体。rCore 有完整 `Siginfo`/`SignalStack`/`SignalActionFlags`；Chaos 简化或省略了这些。

---

## 4. 文件系统（fs → kernel.rs L1619-2360）

### 4.1 文件描述符选项

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct OpenOptions` | `fs/file.rs:51` | read/write/append/nonblock |
| `enum Flock` | `fs/file.rs:19` | None/Shared/Exclusive |
| `enum SeekFrom` | `fs/file.rs:60` | Start(u64)/End(i64)/Current(i64) |
| 常量 `O_NONBLOCK/O_APPEND/O_CLOEXEC` 等 | `fs/fcntl.rs` | fcntl 常量 |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct FdOpt` | `kernel.rs:1619` | rd/wr/ap/nb |
| `enum FSeek` | `kernel.rs:1646` | Start(u64)/End(i64)/Cur(i64) |
| `struct FdState` | `kernel.rs:1629` | off: u64 + opt: FdOpt + flk: u8 |

---

### 4.2 文件句柄

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct FileHandle` | `fs/file.rs:42` | inode + description + path + pipe + fd_cloexec |
| `struct OpenFileDescription` | `fs/file.rs:25` | offset + options + flock |

**FileHandle 方法**:
- `new(inode, options, path, pipe, fd_cloexec)` (L67)
- `dup(fd_cloexec) -> Self` (L84)
- `read(&mut [u8]) -> Result<usize>` (L106)
- `read_at(offset, buf) -> Result<usize>` (L113)
- `write(&[u8]) -> Result<usize>` (L139)
- `write_at(offset, buf) -> Result<usize>` (L151)
- `seek(SeekFrom) -> Result<u64>` (L160)
- `set_len(u64) -> Result<()>` (L170)
- `sync_all() -> Result<()>` (L178)
- `metadata() -> Result<Metadata>` (L186)
- `poll() -> Result<PollStatus>` (L216)
- `io_control(cmd, arg) -> Result<usize>` (L224)
- `mmap(MMapArea) -> Result<()>` (L228)

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct FHandle` | `kernel.rs:1637` | path + data:Arc\<Mutex\<Vec\<u8\>\>\> + desc + pipe + cloexec |

**FHandle 方法**:
- `new(path, opt, pipe, cloexec)` (L1648)
- `with_data(path, opt, data)` (L1658)
- `dup(cloexec) -> Self` (L1667)
- `set_opt(arg)` (L1676), `get_opt() -> FdOpt` (L1680)
- `read(dst) -> Result<usize>` (L1682)
- `read_at(off, dst) -> Result<usize>` (L1688)
- `write(data) -> Result<usize>` (L1703)
- `write_at(off, data) -> Result<usize>` (L1712)
- `seek(pos: FSeek) -> Result<u64>` (L1719)
- `set_len(len) -> Result<()>` (L1750)
- `sync_all()` (L1755), `sync_data()` (L1756)
- `metadata_sz() -> usize` (L1757)
- `lookup/`read_entry`/poll_status/io_ctl/mmap/inode_ref` (L1758-1769)

**关键差异**: rCore 基于 `Arc<dyn INode>`；Chaos 用 `Arc<Mutex<Vec<u8>>>` 存储文件数据（纯内存模拟）。rCore 有 async_poll；Chaos 无异步支持。

---

### 4.3 管道

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Pipe` | `fs/pipe.rs:33` | data: Arc\<Mutex\<PipeData\>\> + direction: PipeEnd |
| `enum PipeEnd` | `fs/pipe.rs:20` | Read/Write |
| `PipeData` | `fs/pipe.rs:25` | buf: VecDeque\<u8\> + eventbus + end_cnt |
- `Pipe::create_pair() -> (Pipe, Pipe)` (L49)
- Pipe 实现 INode trait（read_at/write_at/poll/async_poll）

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `enum PipeDir` | `kernel.rs:1809` | Rd/Wr |
| `struct PipeBuf` | `kernel.rs:1811` | buf: VecDeque\<u8\> + bus: EvBus + ends: i32 |
| `struct PipeNode` | `kernel.rs:1818` | data: Arc\<Mutex\<PipeBuf\>\> + dir: PipeDir |

**PipeNode 方法**: `pair() -> (Self, Self)` (L1831), `can_read() -> bool` (L1840), `can_write() -> bool` (L1845), `read_at/off/buf` (L1849), `write_at/off/buf` (L1859), `poll()` (L1866)

**关键差异**: 结构几乎相同，但 Chaos 用 `PipeNode` + `PipeDir` 而非 `Pipe` + `PipeEnd`。Chaos 不实现 INode trait，而是通过 FLike enum 分发。

---

### 4.4 文件类型枚举

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `enum FileLike` | `fs/file_like.rs:13` | File(FileHandle)/Socket(Box\<dyn Socket\>)/EpollInstance(EpollInstance) |

**FileLike 方法**: `dup(cloexec)`, `read(buf)`, `write(buf)`, `ioctl(request, arg1..3)`, `mmap(area)`, `poll()`, `async_poll()`

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `enum FLike` | `kernel.rs:1872` | File(FHandle)/Pipe(PipeNode)/Ep(EpInst) |

**FLike 方法**: `dup(cloexec)` (L1879), `read(dst)` (L1907), `write(data)` (L1947), `io_ctl(request, args)` (L1990), `mmap_fl(area)` (L2008), `poll()` (L2021)

**关键差异**: rCore 含 Socket 变体；Chaos 用 Pipe 替代。Chaos 无 async_poll。

---

### 4.5 Epoll

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct EpollInstance` | `fs/mod.rs:7` | events + ready_list + new_ctl_list |
| `struct EpollEvent` | `fs/epoll.rs:66` | events: u32 + data: EpollData |
| `EPollCtlOp` | `fs/epoll.rs:98` | ADD/DEL/MOD |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct EpInst` | `kernel.rs:2107` | events + ready + new_ctl（均 BTreeMap/BTreeSet） |
| `struct EpEvent` | `kernel.rs:2079` | events: u32 + data: EpData |
| `struct EpCtlOp` | `kernel.rs:2099` | ADD/DEL/MOD 常量 |

**EpInst 方法**: `new(flags) -> Self` (L2112), `control(op, fd, event) -> Result` (L2120)

---

### 4.6 其他文件系统组件

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct Pseudo` | `fs/pseudo.rs:8` | 伪文件系统 inode |
| `struct MemBuf` | `fs/device.rs:10` | 设备内存缓冲 |
| `struct RandomINode` | `fs/devfs/random.rs` | 随机设备 |
| `struct Serial` | `fs/devfs/serial.rs` | 串口设备 |
| `struct TtyINode` | `fs/devfs/tty.rs` | TTY 设备 |
| `struct Fbdev` | `fs/devfs/fbdev.rs` | 帧缓冲设备 |
| `struct ShmINode` | `fs/devfs/shm.rs` | 共享内存 inode |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct PseudoNode` | `kernel.rs:2060` | content: Vec\<u8\> + ftype: u8 |
| `struct TrmIO` | `kernel.rs:2146` | 终端 I/O（iflag/oflag/lflag/cc） |
| `struct WinSz` | `kernel.rs:2173` | 窗口尺寸 |

**关键差异**: Chaos 大幅简化了设备文件系统，移除了 Serial/Fbdev/Random 等设备 inode，仅保留 PseudoNode 和 TrmIO。

---

## 5. IPC（ipc → kernel.rs L2166-2360, 3128-3290）

### 5.1 System V 信号量

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct SemArray` | `ipc/semary.rs:49` | semid_ds + sems: Vec\<Semaphore\> |
| `struct IpcPerm` | `ipc/semary.rs:21` | key/uid/gid/mode |
| `struct SemidDs` | `ipc/semary.rs:38` | perm + otime + ctime + nsems |
| `SemArray::get_or_create(key, nsems, flags)` | L95 | 获取或创建信号量数组 |
| `struct SemProc` | `ipc/mod.rs:14` | arrays + undos |
| `SemProc::add/remove/get/add_undo` | L38-66 | 进程级信号量管理 |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct SemArr` | `kernel.rs:3151` | ds: Mutex\<SemDs\> + sems: Vec\<Sema\> |
| `struct IpcPerm` | `kernel.rs:3128` | key/uid/gid/mode 等 |
| `struct SemDs` | `kernel.rs:3142` | perm + otime + ctime + nsems |
| `struct SemCtx` | `kernel.rs:3207` | arrs + undos（进程级信号量上下文） |

**SemArr 方法**: `remove()` (L3159), `otime()` (L3162), `ctime()` (L3165), `set(new)` (L3168), `get_or_create(key, nsems, flags)` (L3175)

**SemCtx 方法**: `add(arr)` (L3213), `remove(id)` (L3216), `get(id)` (L3220), `add_undo(id, num, op)` (L3222)

---

### 5.2 共享内存

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `struct ShmProc` | `ipc/mod.rs:23` | shm_identifiers: BTreeMap |
| `struct ShmIdentifier` | `ipc/shared_mem.rs:17` | addr + shared_guard |
| `ShmProc::add/get/set/get_id/pop` | L95-138 |
| `ShmIdentifier::new_shared_guard(key, memsize)` | L27 |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct ShmCtx` | `kernel.rs:3269` | ids: BTreeMap\<ShmId, ShmTag\> |
| `struct ShmTag` | `kernel.rs:3246` | addr + guard |

**ShmCtx 方法**: `add(guard) -> ShmId` (L3270), `get(id)` (L3273), `set(id, tag)` (L3276), `get_id(addr)` (L3279), `pop(id)` (L3282)

---

### 5.3 Channel（Chaos 新增）

Chaos 新增了 `Channel`（L2175）和 `CircBuf`（L259），用于线程间消息传递，rCore 无此概念。

**Channel 方法**: `new(cap)` (L2182), `recv() -> Option<u8>` (L2190), `send(v: u8) -> bool` (L2261), `close()` (L2283), `try_recv()` (L2289), `send_batch(data)` (L2306)

**CircBuf 方法**: `new(cap)` (L1288), `push(v)` (L1293), `pop() -> Option<u8>` (L1306), `len/empty/full` (L1314-1316), `peek()` (L1318), `drain_to/fill_from` (L1325-1333)

---

## 6. 内存管理（memory → kernel.rs L658-1120, 5855-6390）

### 6.1 物理帧管理

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `type MemorySet` | `memory.rs:16` | rcore_memory::MemorySet\<PageTableImpl\> |
| `type FrameAlloc` | `memory.rs:20` | BitAlloc256M / BitAlloc1M |
| `struct GlobalFrameAlloc` | `memory.rs:64` | 全局帧分配器 |
| `fn alloc_frame() -> Option<usize>` | L95 |
| `fn dealloc_frame(target)` | L98 |
| `fn alloc_frame_contiguous(size, align) -> Option<usize>` | L101 |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct FramePool` | `kernel.rs:959` | 位图帧池（slots: Vec\<bool\>） |
| `struct ZoneInfo` | `kernel.rs:249` | 内存区域信息 |
| `struct PgFrame` | `kernel.rs:658` | 引用计数帧 |
| `struct BuddyAllocator` | `kernel.rs:6280` | 伙伴系统分配器 |

**FramePool 方法**: `new(n)` (L964), `get(id)` (L965), `get_inner()` (L971), `get_contig(sz, align)` (L978), `put(idx)` (L990), `avail(idx)` (L994), `free_count()` (L998), `batch_alloc(count)` (L1025)

**BuddyAllocator 方法**: `new(max_order, base, total)` (L6289), `alloc_order(order)` (L6321), `free_order(order, addr)` (L6339), `free_pages_count()` (L6358), `largest_free_order()` (L6366), `snapshot()` (L6382)

---

### 6.2 虚拟地址空间

#### rCore

rCore 依赖 `rcore-memory` crate 提供 `MemorySet`，内核侧主要是:

- `fn handle_page_fault(addr) -> bool` (`memory.rs:134`)
- `fn handle_page_fault_ext(addr, access) -> bool` (`memory.rs:144`)
- `fn init_heap()` / `fn enlarge_heap()` (`memory.rs:155,167`)
- `fn access_ok(addr, len) -> bool` (`memory.rs:196`)
- `fn copy_from_user/copy_to_user` (`memory.rs:205,222`)

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct VmRegion` | `kernel.rs:174` | base/len/flags/offset/tag/ref_count |
| `struct VmMap` | `kernel.rs:752` | regions: Vec\<VmRegion\> + brk + mmap_base |
| `struct AddrSpace` | `kernel.rs:5855` | vm_map + page_table_root + asid + cow_pages |

**VmMap 方法**: `new()` (L759), `insert(region)` (L763), `find(addr)` (L783), `remove_range(base, len)` (L798), `find_free(len, align)` (L816), `total_mapped()` (L844), `clone_regions()` (L852)

**AddrSpace 方法**: `new(asid)` (L5863), `fork_from(parent, asid)` (L5869), `handle_cow_fault(addr)` (L5902), `unmap_range(base, len)` (L5924), `protect(base, len, flags)` (L5940), `split_region(addr)` (L5965)

---

## 7. 陷阱与时钟（trap → kernel.rs L3486-3730）

#### rCore

| 接口 | 位置 | 说明 |
|---|---|---|
| `static TICK: AtomicUsize` | `trap.rs:11` | 时钟计数 |
| `static TICK_ALL_PROCESSORS` | `trap.rs:12` | 全局 tick |
| `fn wall_tick() -> usize` | L14 |
| `fn cpu_tick() -> usize` | L18 |
| `fn do_tick()` | L20 |
| `fn uptime_msec() -> usize` | L30 |
| `static NAIVE_TIMER: Mutex<Timer>` | L35 |
| `fn timer()` / `fn serial(c: u8)` | L38/L46 |

#### Chaos

| 对应 | 位置 | 说明 |
|---|---|---|
| `struct TimerEntry` | `kernel.rs:201` | deadline + interval + callback_id + active + repeat |
| `struct TimerWheel` | `kernel.rs:3511` | slots: Vec\<Vec\<TimerEntry\>\> + current_slot |
| `static CLK: AtomicUsize` | — | 时钟（全局） |
| `struct TrapCtl` | `kernel.rs:3716` | 陷阱控制 |

**TimerWheel 方法**: `new()` (L3516), `tick()` (L3519), `schedule(deadline, interval, cb_id)` (L3525), `cancel(timer_id)` (L3543), `next_deadline()` (L3551), `active_count()` (L3562)

**TimerEntry 方法**: `new(deadline, interval, cb_id)` (L3486), `expired() -> bool` (L3491), `reset()` (L3495), `remaining() -> usize` (L3503), `cancel()` (L3507)

**TrapCtl 方法**: `new()` (L3726), `register_handler(vec, handler)` (L3729), `invoke(vec, ctx)` (L3738), `set_irq_enabled(vec, enabled)` (L3753), `is_irq_enabled(vec)` (L3764), `ack_irq(vec)` (L3769)

---

## 8. 其他 Chaos 新增结构（rCore 无对应）

| 结构 | 位置 | 说明 |
|---|---|---|
| `struct CapSet` | L183 | 能力集合 |
| `struct SlabEntry` | L334 | SLAB 分配器条目 |
| `enum SocketState` | L344 | 套接字状态 |
| `struct SharedPage` | L1135 | 共享页 |
| `struct KStk` | L1176 | 内核栈 |
| `struct PageCache/PageCacheEntry` | L2363 | 页缓存 |
| `struct KObjEntry/KObjRegistry` | L2512 | 内核对象注册 |
| `struct CacheSlot/CacheChain/BlockCache` | L2646 | 块缓存 |
| `struct MountEntry/MountTable` | L2805 | 挂载表 |
| `struct IoRequest/IoQueue` | L2930 | I/O 请求队列 |
| `struct Disk` | L3041 | 磁盘抽象 |
| `struct SchedulePolicy` | L3909 | 调度策略 |
| `struct RunQueue` | L3938 | 运行队列 |
| `struct Kernel` | L4578 | 内核主结构（~1200 行实现） |
| `struct ProcessGroup` | L5975 | 进程组 |
| `struct WaitQueue` | L6042 | 等待队列 |
| `struct ResourceLimits` | L6137 | 资源限制 |

---

## 9. 总结对照表

| rCore 模块 | rCore 文件数 | Chaos 对应 | Chaos 行范围 | 模型变化 |
|---|---|---|---|---|
| `sync/mutex.rs` | 1 | `Spin` + `KernLock` | L210-283 | 泛型 Mutex → 简单 Spin + KernLock |
| `sync/condvar.rs` | 1 | `SyncQueue` | L358-466 | async Condvar → 阻塞 SyncQueue |
| `sync/semaphore.rs` | 1 | `Sema` | L467-526 | async Semaphore → 阻塞 Sema |
| `sync/event_bus.rs` | 1 | `EvBus` + `EvFlag` | L288-327 | bitflags Event → 裸 u32 |
| `process/proc.rs` | 1 | `Task` + `TaskTable` | L4111-4575 | Mutex\<Process\> → 细粒度 Mutex 字段 |
| `process/thread.rs` | 1 | `ThdCtx` + `Context` | L4130, 3572 | Thread 分离 → 合入 Task |
| `process/futex.rs` | 1 | `FutexBucket` + `FutexTable` | L527-640 | async Futex → 两套阻塞实现 |
| `process/structs.rs` | 1 | `ProcInit` | L3287 | 合并简化 |
| `process/abi.rs` | 1 | `ProcInit` | L3287 | 合并简化 |
| `signal/mod.rs` | 1 | `SigSet` + 信号方法 | L3395-3484 | Signal enum → i32 裸数字 |
| `signal/action.rs` | 1 | `SigAction` | L189 | Sigset/Siginfo 简化 |
| `fs/mod.rs` | 1 | `MountTable` 等 | L2805 | 简化 |
| `fs/file.rs` | 1 | `FHandle` | L1637-1806 | INode → Vec\<u8\> 内存模拟 |
| `fs/pipe.rs` | 1 | `PipeNode` | L1818-1869 | 结构类似，无 INode trait |
| `fs/file_like.rs` | 1 | `enum FLike` | L1872-2050 | 含 Pipe 变体，无 Socket |
| `fs/epoll.rs` | 1 | `EpInst` | L2107 | 简化 |
| `fs/device.rs` | 1 | — | — | 移除 |
| `fs/fcntl.rs` | 1 | 常量内联 | 散布 | 合并 |
| `fs/ioctl.rs` | 1 | `TrmIO` + `WinSz` | L2146-2173 | 简化 |
| `fs/pseudo.rs` | 1 | `PseudoNode` | L2060 | 简化 |
| `fs/devfs/*` | 5 | — | — | 移除 |
| `ipc/mod.rs` | 1 | `SemCtx` + `ShmCtx` | L3207-3290 | 结构类似 |
| `ipc/semary.rs` | 1 | `SemArr` | L3151 | Semaphore → Sema |
| `ipc/shared_mem.rs` | 1 | `ShmTag` + `ShmCtx` | L3246-3290 | 结构类似 |
| `memory.rs` | 1 | `FramePool` + `AddrSpace` + `BuddyAllocator` | L658-1120, 5855-6390 | rcore-memory crate → 自实现 |
| `trap.rs` | 1 | `TimerWheel` + `TrapCtl` | L3486-3769 | naive-timer → TimerWheel |

**总计**: rCore 28 个源文件 → chaos 1 个 kernel.rs 文件

---

## 10. 系统调用对比（syscall）

Chaos 存在**两套** syscall 分发路径：

1. **`kernel.rs::Kernel::dispatch_syscall`**（L4704-5507）— chaos 自写的简化模拟实现，用于用户态测试
2. **`syscall/mod.rs::Syscall::syscall`**（L120-456）— 继承自 rCore，保持原样（与 rCore 完全一致）

下面按类别对比 rCore 和 chaos 各自实现的 syscall。

### 10.1 文件 I/O

| Syscall | # | rCore (`syscall/`) | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_READ` | 0 | `sys_read` | L4704 | chaos: 从 FHandle/FLike 读取 |
| `SYS_WRITE` | 1 | `sys_write` | L4735 | chaos: 写入 FHandle/FLike |
| `SYS_OPEN` | 2 | — | L4767 | chaos 独有（rCore 用 OPENAT） |
| `SYS_OPENAT` | 257 | `sys_openat` | — | rCore 独有 |
| `SYS_CLOSE` | 3 | `sys_close` | L4834 | 两者都有 |
| `SYS_STAT` | 4 | `sys_stat` | L4856 | chaos: STAT/FSTAT 合并处理 |
| `SYS_FSTAT` | 5 | `sys_fstat` | L4856 | 同上 |
| `SYS_LSEEK` | 8 | `sys_lseek` | — | 仅 syscall/mod.rs |
| `SYS_IOCTL` | 16 | `sys_ioctl` | L4944 | 两者都有 |
| `SYS_PREAD64` | 17 | `sys_pread` | — | 仅 syscall/mod.rs |
| `SYS_PWRITE64` | 18 | `sys_pwrite` | — | 仅 syscall/mod.rs |
| `SYS_READV` | 19 | `sys_readv` | — | 仅 syscall/mod.rs |
| `SYS_WRITEV` | 20 | `sys_writev` | — | 仅 syscall/mod.rs |
| `SYS_SENDFILE` | 40 | `sys_sendfile` | — | 仅 syscall/mod.rs |
| `SYS_PIPE` | 22 | `sys_pipe` | L4978 | 两者都有 |
| `SYS_PIPE2` | 293 | `sys_pipe2` | — | 仅 syscall/mod.rs |
| `SYS_DUP` | 32 | — | L4997 | chaos dispatch 独有 |
| `SYS_DUP2` | 33 | `sys_dup2` | L5011 | 两者都有 |
| `SYS_DUP3` | 292 | `sys_dup3` | — | 仅 syscall/mod.rs |
| `SYS_FCNTL` | 72 | `sys_fcntl` | L5230 | 两者都有 |
| `SYS_FLOCK` | 32 | `sys_flock` | — | 仅 syscall/mod.rs |
| `SYS_FSYNC` | 74 | `sys_fsync` | — | 仅 syscall/mod.rs |
| `SYS_FDATASYNC` | 75 | `sys_fdatasync` | — | 仅 syscall/mod.rs |
| `SYS_TRUNCATE` | 76 | `sys_truncate` | — | 仅 syscall/mod.rs |
| `SYS_FTRUNCATE` | 77 | `sys_ftruncate` | — | 仅 syscall/mod.rs |
| `SYS_GETDENTS64` | 61/217 | `sys_getdents64` | — | 仅 syscall/mod.rs |
| `SYS_GETCWD` | 79 | `sys_getcwd` | — | 仅 syscall/mod.rs |
| `SYS_CHDIR` | 80 | `sys_chdir` | — | 仅 syscall/mod.rs |
| `SYS_RENAMEAT` | 264 | `sys_renameat` | — | 仅 syscall/mod.rs |
| `SYS_MKDIRAT` | 258 | `sys_mkdirat` | — | 仅 syscall/mod.rs |
| `SYS_LINKAT` | 265 | `sys_linkat` | — | 仅 syscall/mod.rs |
| `SYS_UNLINKAT` | 263 | `sys_unlinkat` | — | 仅 syscall/mod.rs |
| `SYS_SYMLINKAT` | 266 | `sys_symlinkat` | — | 仅 syscall/mod.rs |
| `SYS_READLINKAT` | 267 | `sys_readlinkat` | — | 仅 syscall/mod.rs |
| `SYS_FACCESSAT` | 269 | `sys_faccessat` | — | 仅 syscall/mod.rs |
| `SYS_UTIMENSAT` | 280 | `sys_utimensat` | — | 仅 syscall/mod.rs |
| `SYS_COPY_FILE_RANGE` | 326 | `sys_copy_file_range` | — | 仅 syscall/mod.rs |

### 10.2 内存管理

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_MMAP` | 9 | `sys_mmap` | L4872 | chaos: 调用 VmMap 操作 |
| `SYS_MUNMAP` | 11 | `sys_munmap` | L4906 | 两者都有 |
| `SYS_MPROTECT` | 10 | `sys_mprotect` | — | 仅 syscall/mod.rs |
| `SYS_BRK` | 12 | unimplemented | L4917 | chaos: 简化实现 |
| `SYS_MADVISE` | 28 | unimplemented | — | 仅 syscall/mod.rs 标记 unimplemented |

### 10.3 进程管理

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_FORK` | 57 | `sys_fork` | L5030 | chaos: 调用 TaskTable::fork_task |
| `SYS_VFORK` | 58 | `sys_vfork` | — | 仅 syscall/mod.rs |
| `SYS_CLONE` | 56 | `sys_clone` | — | 仅 syscall/mod.rs |
| `SYS_EXECVE` | 59 | `sys_exec` | L5053 | chaos: 简化实现 |
| `SYS_EXIT` | 60 | `sys_exit` | L5074 | chaos: 调用 Task::exit_proc |
| `SYS_EXIT_GROUP` | 231 | `sys_exit_group` | — | 仅 syscall/mod.rs |
| `SYS_WAIT4` | 61 | `sys_wait4` | L5096 | chaos: 简化实现 |
| `SYS_GETPID` | 39 | `sys_getpid` | L5287 | 两者都有 |
| `SYS_GETPPID` | 110 | `sys_getppid` | L5294 | 两者都有 |
| `SYS_GETTID` | 186 | `sys_gettid` | — | 仅 syscall/mod.rs |
| `SYS_SETPGID` | 109 | `sys_setpgid` | L5307 | 两者都有 |
| `SYS_GETPGID` | 121 | `sys_getpgid` | L5331 | 两者都有 |
| `SYS_SETSID` | 112 | `sys_setsid` | L5345 | 两者都有 |
| `SYS_SET_TID_ADDRESS` | 218 | `sys_set_tid_address` | — | 仅 syscall/mod.rs |
| `SYS_SCHED_YIELD` | 24 | `sys_yield` | — | 仅 syscall/mod.rs |
| `SYS_SCHED_GETAFFINITY` | 204 | `sys_sched_getaffinity` | — | 仅 syscall/mod.rs |

### 10.4 信号处理

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_SIGACTION` | 13 | `sys_rt_sigaction` | L5427 | chaos: 简化（仅检查权限，不实际设置） |
| `SYS_SIGPROCMASK` | 14 | `sys_rt_sigprocmask` | L5439 | chaos: 操作 Task.sig_mask |
| `SYS_RT_SIGACTION` | 13 | `sys_rt_sigaction` | — | 仅 syscall/mod.rs |
| `SYS_RT_SIGPROCMASK` | 14 | `sys_rt_sigprocmask` | — | 仅 syscall/mod.rs |
| `SYS_RT_SIGRETURN` | 15 | `sys_rt_sigreturn` | — | 仅 syscall/mod.rs |
| `SYS_SIGALTSTACK` | 131 | `sys_sigaltstack` | — | 仅 syscall/mod.rs |
| `SYS_KILL` | 62 | `sys_kill` | L5182 | chaos: 发送到任务信号队列 |
| `SYS_TKILL` | 200 | `sys_tkill` | — | 仅 syscall/mod.rs |
| `SYS_TGKILL` | 234 | — | — | 仅 syscall/mod.rs 标记 unimplemented |

### 10.5 IPC

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_FUTEX` | 202 | `sys_futex` (async) | L5465 | chaos: 简化模拟（返回占位值） |
| `SYS_SEMGET` | 64 | `sys_semget` | — | 仅 syscall/mod.rs |
| `SYS_SEMOP` | 65 | `sys_semop` (async) | — | 仅 syscall/mod.rs |
| `SYS_SEMCTL` | 66 | `sys_semctl` | — | 仅 syscall/mod.rs |
| `SYS_SHMGET` | 29 | `sys_shmget` | — | 仅 syscall/mod.rs |
| `SYS_SHMAT` | 30 | `sys_shmat` | — | 仅 syscall/mod.rs |
| `SYS_SHMDT` | 67 | `sys_shmdt` | — | 仅 syscall/mod.rs |
| `SYS_MSGGET` | 68 | unimplemented | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_MSGCTL` | 71 | unimplemented | — | 仅 syscall/mod.rs 标记 unimplemented |

### 10.6 网络（Socket）

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_SOCKET` | 41 | `sys_socket` | — | 仅 syscall/mod.rs |
| `SYS_BIND` | 49 | `sys_bind` | — | 仅 syscall/mod.rs |
| `SYS_CONNECT` | 42 | `sys_connect` | — | 仅 syscall/mod.rs |
| `SYS_ACCEPT` | 43 | `sys_accept` | — | 仅 syscall/mod.rs |
| `SYS_ACCEPT4` | 288 | `sys_accept` | — | 仅 syscall/mod.rs |
| `SYS_LISTEN` | 50 | `sys_listen` | — | 仅 syscall/mod.rs |
| `SYS_SHUTDOWN` | 48 | `sys_shutdown` | — | 仅 syscall/mod.rs |
| `SYS_SENDTO` | 44 | `sys_sendto` | — | 仅 syscall/mod.rs |
| `SYS_RECVFROM` | 45 | `sys_recvfrom` | — | 仅 syscall/mod.rs |
| `SYS_RECVMSG` | 47 | `sys_recvmsg` | — | 仅 syscall/mod.rs |
| `SYS_GETSOCKNAME` | 51 | `sys_getsockname` | — | 仅 syscall/mod.rs |
| `SYS_GETPEERNAME` | 52 | `sys_getpeername` | — | 仅 syscall/mod.rs |
| `SYS_SETSOCKOPT` | 54 | `sys_setsockopt` | — | 仅 syscall/mod.rs |
| `SYS_GETSOCKOPT` | 55 | `sys_getsockopt` | — | 仅 syscall/mod.rs |

### 10.7 epoll / I/O 多路复用

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_EPOLL_CREATE` | 213 | `sys_epoll_create` | L5359 | chaos: 创建 EpInst |
| `SYS_EPOLL_CREATE1` | 291 | `sys_epoll_create1` | — | 仅 syscall/mod.rs |
| `SYS_EPOLL_CTL` | 233 | `sys_epoll_ctl` | L5367 | chaos: 调用 EpInst::control |
| `SYS_EPOLL_WAIT` | 232 | `sys_epoll_wait` | L5382 | chaos: 简化实现 |
| `SYS_EPOLL_PWAIT` | 281 | `sys_epoll_pwait` | — | 仅 syscall/mod.rs |
| `SYS_PSELECT6` | 270 | `sys_pselect6` | — | 仅 syscall/mod.rs |
| `SYS_PPOLL` | 271 | `sys_ppoll` | — | 仅 syscall/mod.rs |

### 10.8 时间

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_CLOCK_GETTIME` | 228 | `sys_clock_gettime` | L5401 | chaos: 从 CLK 读取 ticks |
| `SYS_NANOSLEEP` | 35 | `sys_nanosleep` (async) | — | 仅 syscall/mod.rs |
| `SYS_GETTIMEOFDAY` | 96 | `sys_gettimeofday` | — | 仅 syscall/mod.rs |
| `SYS_SETITIMER` | 36 | unimplemented | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_TIMES` | 100 | `sys_times` | — | 仅 syscall/mod.rs |
| `SYS_TIME` | 201 | `sys_time` | — | 仅 syscall/mod.rs |
| `SYS_GETRUSAGE` | 98 | `sys_getrusage` | — | 仅 syscall/mod.rs |

### 10.9 系统 / 杂项

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_UNAME` | 63 | `sys_uname` | — | 仅 syscall/mod.rs |
| `SYS_SYSINFO` | 99 | `sys_sysinfo` | — | 仅 syscall/mod.rs |
| `SYS_REBOOT` | 169 | `sys_reboot` | — | 仅 syscall/mod.rs |
| `SYS_PRLIMIT64` | 302 | `sys_prlimit64` | — | 仅 syscall/mod.rs |
| `SYS_GETRANDOM` | 318 | `sys_getrandom` | — | 仅 syscall/mod.rs |
| `SYS_ARCH_PRCTL` | 158 | `sys_arch_prctl` | — | 仅 syscall/mod.rs (x86_64) |
| `SYS_SET_ROBUST_LIST` | 273 | — | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_GET_ROBUST_LIST` | 274 | — | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_SYNC` | 162 | `sys_sync` | — | 仅 syscall/mod.rs |
| `SYS_EVENTFD2` | 290 | — | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_SOCKETPAIR` | 206 | — | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_STATFS` | 137 | — | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_FSTATFS` | 138 | — | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_MOUNT` | 165 | — | — | 仅 syscall/mod.rs 标记 unimplemented |
| `SYS_UMOUNT2` | 166 | — | — | 仅 syscall/mod.rs 标记 unimplemented |

### 10.10 内核模块（LKM）

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_INIT_MODULE` | 175 | `sys_init_module` | — | 仅 syscall/mod.rs |
| `SYS_FINIT_MODULE` | 313 | unimplemented | — | 仅 syscall/mod.rs |
| `SYS_DELETE_MODULE` | 176 | `sys_delete_module` | — | 仅 syscall/mod.rs |

### 10.11 自定义 Syscall

| Syscall | # | rCore | Chaos `dispatch_syscall` | 说明 |
|---|---|---|---|---|
| `SYS_MAP_PCI_DEVICE` | 999 | `sys_map_pci_device` | — | 仅 syscall/mod.rs |
| `SYS_GET_PADDR` | 998 | `sys_get_paddr` | — | 仅 syscall/mod.rs |

### 10.12 总结

**rCore `syscall/mod.rs`**: ~90+ 个 syscall 条目，其中约 50+ 个有实际实现，其余标记 `unimplemented`。全部使用 async/await。

**Chaos `dispatch_syscall`**: 30 个 syscall 常量定义（L140-170），约 30 个 case 分支（L4704-5507），其中部分为简化实现：

| 类别 | chaos `dispatch` 实现数 | 说明 |
|---|---|---|
| 文件 I/O | 10 | read/write/open/close/stat/ioctl/pipe/dup/dup2/fcntl |
| 内存管理 | 3 | mmap/munmap/brk |
| 进程管理 | 8 | fork/exec/exit/wait4/getpid/getppid/setpgid/getpgid/setsid |
| 信号 | 3 | sigaction/sigprocmask/kill |
| IPC | 1 | futex（简化模拟） |
| epoll | 3 | create/ctl/wait |
| 时间 | 1 | clock_gettime |
| **总计** | **~30** | |

**关键差异**:

1. **两套系统共存**: chaos 的 `kernel.rs::dispatch_syscall` 是独立的简化模拟层，用于 std 测试环境；`syscall/mod.rs` 继承自 rCore，用于真实内核运行。两者互不调用。

2. **chaos dispatch 是模拟**: 多数实现只做参数校验和状态更新，不真正执行底层操作（如 `SYS_SIGACTION` 不实际设置 handler，`SYS_FUTEX` 返回占位值）。

3. **rCore 用 async/await**: `syscall/mod.rs` 中许多 syscall（nanosleep、semop、futex、wait4 等）使用 async，依赖内核的 future executor 调度。chaos `dispatch_syscall` 全部是同步阻塞。

4. **chaos 独有**: `SYS_OPEN`（#2）直接在 dispatch 中实现（rCore 用 `SYS_OPENAT` 替代），`SYS_DUP`（#32）等。

5. **chaos 缺失的类别**: 网络相关 14 个 syscall、大部分文件系统操作（mkdirat/linkat/unlinkat 等）、时间类（nanosleep/gettimeofday）均未在 `dispatch_syscall` 中实现，仅保留在 `syscall/mod.rs` 中。

---

## 11. 模块拆分依赖关系分析

将 kernel.rs 拆回 rCore 式的模块结构时，需要处理模块间的类型依赖。以下是各模块的具体引用关系。

### 11.1 依赖矩阵

| ↓ 依赖于 → | sync | signal | memory | fs | ipc | process | trap | kernel |
|---|---|---|---|---|---|---|---|---|
| **sync** | — | | | | | | | |
| **signal** | Mutex | — | | | | | | |
| **memory** | Mutex, AtomicBool | | — | | | | | |
| **fs** | Spin, EvBus, EvFlag, SyncQueue | | | — | | | | |
| **ipc** | Spin, SyncQueue | | | | — | | | |
| **trap** | Mutex, AtomicBool | | | | | | — | |
| **process** | EvBus, FutexBucket, Sema, KernLock | SigSet, SigAction | AddrSpace, KStk, PgFrame | FLike, EpInst, FHandle | SemCtx, ShmCtx | — | Context, TimerWheel | |
| **kernel** | Mutex | | FramePool | BlockCache, MountTable | SemArr (Weak) | TaskTable, Task | | — |

### 11.2 具体字段级依赖

#### Task 结构体（process 模块）— 依赖最多的结构体

```
Task {
    // → sync 模块
    ev: Arc<Mutex<EvBus>>,           // EvBus 来自 sync
    sig_queue: Mutex<VecDeque<(i32, isize)>>,

    // → signal 模块
    sig_mask: Mutex<u64>,            // 信号掩码（直接用 u64，非 SigSet）

    // → fs 模块
    files: Mutex<BTreeMap<usize, FLike>>,   // FLike 来自 fs
    cwd: Mutex<String>,
    exec_path: Mutex<String>,
    ep_inst: Mutex<BTreeMap<usize, EpInst>>, // EpInst 来自 fs

    // → memory 模块
    kstk: Mutex<Option<KStk>>,       // KStk 来自 memory
    vm_token: AtomicUsize,

    // → ipc 模块
    sem_ctx: Mutex<SemCtx>,          // SemCtx 来自 ipc
    shm_ctx: Mutex<ShmCtx>,          // ShmCtx 来自 ipc
    futexes: Mutex<BTreeMap<usize, Arc<FutexBucket>>>,  // FutexBucket 来自 sync
}
```

#### Kernel 结构体（kernel 模块）

```
Kernel {
    // → process 模块
    tasks: TaskTable,                // TaskTable 来自 process
    cpus: Mutex<[Option<Arc<Task>>; MAX_CPU]>,  // Task 来自 process

    // → fs 模块
    cache: BlockCache,               // BlockCache 来自 fs
    mnt: MountTable,                 // MountTable 来自 fs
    disk: Disk,                      // Disk 来自 fs

    // → memory 模块
    pool: FramePool,                 // FramePool 来自 memory

    // → ipc 模块
    sem_store: RwLock<BTreeMap<u32, Weak<SemArr>>>,  // SemArr 来自 ipc
}
```

#### FLike 枚举（fs 模块）— fs 内部聚合

```
FLike {
    File(FHandle),      // FHandle 来自 fs 内部
    Pipe(PipeNode),     // PipeNode 来自 fs 内部
    Ep(EpInst),         // EpInst 来自 fs 内部
}
```

#### Channel（ipc 模块）→ sync

```
Channel {
    buf: Mutex<CircBuf>,   // CircBuf 来自 ipc 内部
    guard: Spin,            // Spin 来自 sync
    wq: SyncQueue,          // SyncQueue 来自 sync
    shut: AtomicBool,
}
```

#### PipeBuf / BlockCache（fs 模块）→ sync

```
PipeBuf {
    buf: VecDeque<u8>,
    bus: EvBus,              // EvBus 来自 sync
    ends: i32,
}

CacheChain {
    lk: Spin,                // Spin 来自 sync
    items: Mutex<Vec<CacheSlot>>,
}
```

### 11.3 依赖层级（拆分顺序建议）

从底层到上层，无环依赖的拓扑排序：

```
第 0 层（无依赖）：  constants（PAGE_SZ, N_PROC 等）
                     net（tcp_checksum 等纯函数）

第 1 层（仅依赖第 0 层）：sync
    Spin, KernLock, SyncQueue, Sema, EvBus, EvFlag, FutexBucket, FutexTable

第 2 层（依赖 sync）：signal
    SigAction, SigSet
    trap
    TimerEntry, TimerWheel, TrapCtl, Context

第 3 层（依赖 sync + signal）：memory
    VmRegion, VmMap, FramePool, ZoneInfo, PgFrame, AddrSpace,
    BuddyAllocator, KStk, SharedPage

第 4 层（依赖 sync）：fs
    FdOpt, FHandle, PipeBuf, PipeNode, FLike, EpInst, PseudoNode,
    TrmIO, MountTable, BlockCache, IoQueue, Disk
    ipc
    Channel, CircBuf, SemArr, SemCtx, ShmCtx

第 5 层（依赖 sync + signal + memory + fs + ipc + trap）：process
    CapSet, TaskInfo, Task, TaskTable, Pid, ThdCtx,
    ProcessGroup, WaitQueue, ResourceLimits, RunQueue, SchedulePolicy

第 6 层（依赖所有）：kernel
    Kernel + dispatch_syscall + schedule_tick
```

### 11.4 循环依赖问题

**存在循环依赖**：

1. **fs ↔ process**：Task.files 字段持有 `FLike`，而 fs 的 syscall 实现（在 Kernel::dispatch_syscall 中）需要操作 Task。如果拆分后 fs 的 syscall handler 放在 kernel 层而非 fs 模块内，可避免此循环。

2. **sync → process**（潜在）：rCore 原始设计中 Condvar 的 `wait_queue` 持有 `Arc<Thread>`。Chaos 的 SyncQueue 用 `thread::Thread`（std 类型），不直接依赖 process 模块。当前无循环。

**解决方案**：将 syscall dispatch 保留在 kernel 模块中，fs/ipc 模块只提供数据结构和基础操作，不直接操作 Task。fs ↔ process 的交互通过 kernel 层中转。

### 11.5 拆分时的 use 关系预估

拆分后各模块的 `use` 语句大致为：

```rust
// sync/mod.rs
use std::sync::Mutex;  // std Mutex
use std::sync::atomic::*;

// signal/mod.rs
use crate::sync::Mutex;

// trap/mod.rs
use crate::sync::Mutex;
use std::sync::atomic::*;

// memory/mod.rs
use crate::sync::Mutex;
use std::sync::atomic::*;

// fs/mod.rs
use crate::sync::{Spin, EvBus, EvFlag, SyncQueue};

// ipc/mod.rs
use crate::sync::{Spin, SyncQueue};

// process/mod.rs
use crate::sync::{EvBus, FutexBucket, Mutex};
use crate::signal::{SigSet, SigAction};
use crate::memory::{AddrSpace, KStk};
use crate::fs::{FLike, EpInst};
use crate::ipc::{SemCtx, ShmCtx};
use crate::trap::{Context, TimerWheel};

// kernel/mod.rs
use crate::sync::Mutex;
use crate::process::{Task, TaskTable};
use crate::fs::{BlockCache, MountTable, Disk};
use crate::memory::FramePool;
use crate::ipc::SemArr;
```
