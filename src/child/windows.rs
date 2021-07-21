use std::{
	io::{Read, Result},
	mem,
	process::{Child, ChildStderr, ChildStdin, ChildStdout, ExitStatus},
};
use winapi::{
	shared::{basetsd::ULONG_PTR, minwindef::DWORD},
	um::{
		handleapi::CloseHandle,
		ioapiset::GetQueuedCompletionStatus,
		jobapi2::TerminateJobObject,
		minwinbase::LPOVERLAPPED,
		winbase::INFINITE,
		winnt::{HANDLE, JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO},
	},
};

use crate::winres::*;

pub(super) struct ChildImp {
	inner: Child,
	job: HANDLE,
	completion_port: HANDLE,
}

impl Drop for ChildImp {
	fn drop(&mut self) {
		unsafe { CloseHandle(self.job) };
		unsafe { CloseHandle(self.completion_port) };
	}
}

unsafe impl Send for ChildImp {}
unsafe impl Sync for ChildImp {}

impl ChildImp {
	pub fn new(inner: Child, job: HANDLE, completion_port: HANDLE) -> Self {
		Self {
			inner,
			job,
			completion_port,
		}
	}

	pub(super) fn take_stdin(&mut self) -> Option<ChildStdin> {
		self.inner.stdin.take()
	}

	pub(super) fn take_stdout(&mut self) -> Option<ChildStdout> {
		self.inner.stdout.take()
	}

	pub(super) fn take_stderr(&mut self) -> Option<ChildStderr> {
		self.inner.stderr.take()
	}

	pub fn inner(&mut self) -> &mut Child {
		&mut self.inner
	}

	pub fn into_inner(self) -> Child {
		let mut its = mem::ManuallyDrop::new(self);

		// manually drop the completion port
		unsafe { CloseHandle(its.completion_port) };

		// forget about the job -- this leaks a handle, but otherwise the child is useless
		mem::forget(its.job);

		// extract the Child
		let fake_inner = unsafe { mem::zeroed() };
		mem::replace(&mut its.inner, fake_inner)
	}

	pub fn kill(&mut self) -> Result<()> {
		res_bool(unsafe { TerminateJobObject(self.job, 1) })
	}

	pub fn id(&self) -> u32 {
		self.inner.id()
	}

	fn wait_imp(&self, timeout: DWORD) -> Result<bool> {
		let mut code: DWORD = 0;
		let mut key: ULONG_PTR = 0;
		let mut overlapped = mem::MaybeUninit::<LPOVERLAPPED>::uninit();

		res_bool(unsafe {
			GetQueuedCompletionStatus(
				self.completion_port,
				&mut code,
				&mut key,
				overlapped.as_mut_ptr(),
				timeout,
			)
		})?;

		Ok(code == JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO && (key as HANDLE) == self.job)
	}

	pub fn wait(&mut self) -> Result<ExitStatus> {
		self.wait_imp(INFINITE)?;
		self.inner.wait()
	}

	pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
		self.wait_imp(0)?;
		self.inner.try_wait()
	}

	pub(super) fn read_both(
		mut out_r: ChildStdout,
		out_v: &mut Vec<u8>,
		mut err_r: ChildStderr,
		err_v: &mut Vec<u8>,
	) -> Result<()> {
		out_r.read_to_end(out_v)?;
		err_r.read_to_end(err_v)?;
		Ok(())
	}
}
