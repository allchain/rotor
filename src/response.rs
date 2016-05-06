use std::fmt::Debug;
use std::error::Error;

use mio::Token;

use {Response, Time};


#[derive(Debug)]
pub enum ResponseImpl<M, N> {
    Normal(M),
    Deadline(M, Time),
    Spawn(M, N),
    Error(Box<Error>),
    Done,
}

impl<M: Sized, N:Sized> Response<M, N> {
    pub fn ok(machine: M) -> Response<M, N> {
        Response(ResponseImpl::Normal(machine))
    }
    pub fn spawn(machine: M, result: N) -> Response<M, N> {
        Response(ResponseImpl::Spawn(machine, result))
    }
    pub fn done() -> Response<M, N> {
        Response::<M, N>(ResponseImpl::Done)
    }

    /// Stop the state machine with an error.
    ///
    /// If `rotor` was compiled with the `log_errors` feature, the error will
    /// be logged on the warning level.

    // TODO add this documentation once mio has upgraded to slab 0.2.0
    // Additionally, if this response is returned from `Machine::create`,
    // the error is passed to `Machine::spawn_error`.
    pub fn error(e: Box<Error>) -> Response<M, N> {
        Response::<M, N>(ResponseImpl::Error(e))
    }

    pub fn deadline(self, time: Time) -> Response<M, N> {
        let imp = match self.0 {
            ResponseImpl::Normal(x) => ResponseImpl::Deadline(x, time),
            ResponseImpl::Deadline(x, _) => ResponseImpl::Deadline(x, time),
            ResponseImpl::Spawn(..) => {
                panic!("You can't attach a deadline/timeout to the \
                    Response::spawn(). The `spawn` action is synchronous \
                    you must set a deadline in the `spawned` handler."); }
            ResponseImpl::Done => {
                panic!("You can't attach a deadline/timeout to \
                    Response::done() as it's useless. \
                    Timeout will never happen");
            }
            ResponseImpl::Error(_) => {
                panic!("You can't attach a deadline/timeout to \
                    Response::error(_) as it's useless. \
                    Timeout will never happen");
            }
        };
        Response(imp)
    }
    /// Maps state machine and/or spawned result with a function
    ///
    /// Usually it's okay to use constructor of wrapper state machine
    /// here as a mapper
    pub fn map<T, U,  S, R>(self, self_mapper: S, result_mapper: R)
        -> Response<T, U>
        where S: FnOnce(M) -> T,
              R: FnOnce(N) -> U,
    {
        use self::ResponseImpl::*;
        let imp = match self.0 {
            Normal(m) => Normal(self_mapper(m)),
            Deadline(m, time) => Deadline(self_mapper(m), time),
            Spawn(m, n) => Spawn(self_mapper(m), result_mapper(n)),
            Done => Done,
            Error(e) => Error(e),
        };
        Response(imp)
    }
    /// Similar to `map` but only maps state machine
    ///
    /// This is especially useful in state machine constructors, which
    /// have a Void child type.
    pub fn wrap<T, S>(self, self_mapper: S) -> Response<T, N>
        where S: FnOnce(M) -> T
    {
        use self::ResponseImpl::*;
        let imp = match self.0 {
            Normal(m) => Normal(self_mapper(m)),
            Deadline(m, time) => Deadline(self_mapper(m), time),
            Spawn(m, n) => Spawn(self_mapper(m), n),
            Done => Done,
            Error(e) => Error(e),
        };
        Response(imp)
    }

    /// Returns true if state machine is stopped
    ///
    /// I.e. the method returns true if the `Response` was created either with
    /// `Response::done` or `Response::error`
    pub fn is_stopped(&self) -> bool {
        use self::ResponseImpl::*;
        match self.0 {
            Normal(..) => false,
            Deadline(..) => false,
            Spawn(..) => false,
            Done => true,
            Error(..) => true,
        }
    }

    /// Return a reference to an error passed to `Response::error`
    ///
    /// Returns None if any other constructor was used.
    ///
    /// This is mostly useful for printing the error.
    pub fn cause(&self) -> Option<&Error> {
        use self::ResponseImpl::*;
        match self.0 {
            Normal(..) => None,
            Deadline(..) => None,
            Spawn(..) => None,
            Done => None,
            Error(ref e) => Some(&**e),
        }
    }
}

impl<M: Sized + Debug, N: Sized + Debug> Response<M, N> {
    /// Return state machine if response created with `Response::ok(..)`
    ///
    /// *Use only for unit tests*
    ///
    /// If the response is not okay, the function panics.
    pub fn expect_machine(self) -> M {
        match self.0 {
            ResponseImpl::Normal(x) => x,
            ResponseImpl::Deadline(x, _) => x,
            me => panic!("expected machine (`Response::ok(x)`), \
                got {:?} instead", me),
        }
    }
    /// Return a tuple if response created with `Response::spawn(..)`
    ///
    /// *Use only for unit tests*
    ///
    /// If the response is not `spawn`, the function panics.
    pub fn expect_spawn(self) -> (M, N) {
        match self.0 {
            ResponseImpl::Spawn(x, y) => (x, y),
            me => panic!("expected spawn (`Response::spawn(x)`), \
                got {:?} instead", me),
        }
    }
    /// Returns if response created with `Response::done()`
    ///
    /// *Use only for unit tests*
    ///
    /// If the response is not done, the function panics.
    pub fn expect_done(self) {
        match self.0 {
            ResponseImpl::Done => {}
            me => panic!("expected done (`Response::done()`), \
                got {:?} instead", me),
        }
    }
    /// Returns an error if response created with `Response::error(..)`
    ///
    /// *Use only for unit tests*
    ///
    /// If the response does not contain error, the function panics.
    pub fn expect_error(self) -> Box<Error> {
        match self.0 {
            ResponseImpl::Error(e) => e,
            me => panic!("expected error (`Response::error(e)`), \
                got {:?} instead", me),
        }
    }
}

pub fn decompose<M, N>(token: Token, res: Response<M, N>)
    -> (Result<M, Option<Box<Error>>>, Option<N>, Option<Time>)
{
    match res.0 {
        ResponseImpl::Normal(m) => (Ok(m), None, None),
        ResponseImpl::Deadline(m, time) => (Ok(m), None, Some(time)),
        ResponseImpl::Spawn(m, n) => (Ok(m), Some(n), None),
        ResponseImpl::Done => (Err(None), None, None),
        ResponseImpl::Error(e) => {
            if cfg!(feature = "log_errors") {
                warn!("State machine {:?} exited with error: {}", token, e);
            }
            (Err(Some(e)), None, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Response;

    #[test]
    fn size_of_response() {
        assert_eq!(::std::mem::size_of::<Response<u64, u64>>(), 24)
    }
}
