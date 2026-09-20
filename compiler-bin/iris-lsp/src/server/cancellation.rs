use std::collections::HashMap;
use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_lsp::{
    AnyEvent, AnyNotification, AnyRequest, ErrorCode, LspService, RequestId, ResponseError,
};
use futures::stream::{AbortHandle, Abortable};
use lsp_types::notification::{self, Notification};
use tower::{Layer, Service};

/// Adds request cancellation without blocking admission of protocol messages.
///
/// async-lsp's concurrency middleware stops polling the protocol loop when its
/// request limit is full. Workspace requests can deliberately remain pending
/// while preparation runs, so their execution limit belongs inside those
/// request futures instead.
pub(super) struct Cancellation<S> {
    service: S,
    ongoing: HashMap<RequestId, AbortHandle>,
}

pub(super) struct CancellationLayer;

impl<S> Layer<S> for CancellationLayer {
    type Service = Cancellation<S>;

    fn layer(&self, service: S) -> Cancellation<S> {
        Cancellation { service, ongoing: HashMap::new() }
    }
}

impl<S> Service<AnyRequest> for Cancellation<S>
where
    S: LspService,
    S::Error: From<ResponseError>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = CancellationFuture<S::Future>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(context)
    }

    fn call(&mut self, request: AnyRequest) -> Self::Future {
        let (handle, registration) = AbortHandle::new_pair();
        self.ongoing.retain(|_, handle| !handle.is_aborted());
        self.ongoing.insert(request.id.clone(), handle.clone());
        let future = Box::pin(Abortable::new(self.service.call(request), registration));
        CancellationFuture { future, _abort_on_drop: AbortOnDrop(handle) }
    }
}

struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) struct CancellationFuture<F> {
    future: Pin<Box<Abortable<F>>>,
    _abort_on_drop: AbortOnDrop,
}

impl<F, Response, Error> Future for CancellationFuture<F>
where
    F: Future<Output = Result<Response, Error>>,
    Error: From<ResponseError>,
{
    type Output = Result<Response, Error>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.future.as_mut().poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(_)) => Poll::Ready(Err(ResponseError::new(
                ErrorCode::REQUEST_CANCELLED,
                "Client cancelled the request",
            )
            .into())),
        }
    }
}

impl<S> LspService for Cancellation<S>
where
    S: LspService,
    S::Error: From<ResponseError>,
{
    fn notify(&mut self, notification: AnyNotification) -> ControlFlow<async_lsp::Result<()>> {
        if notification.method == notification::Cancel::METHOD {
            if let Ok(parameters) =
                serde_json::from_value::<lsp_types::CancelParams>(notification.params)
                && let Some(handle) = self.ongoing.remove(&parameters.id)
            {
                handle.abort();
            }
            return ControlFlow::Continue(());
        }
        self.service.notify(notification)
    }

    fn emit(&mut self, event: AnyEvent) -> ControlFlow<async_lsp::Result<()>> {
        self.service.emit(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::{BoxFuture, pending, poll_fn};
    use serde_json::json;

    struct PendingService;

    impl Service<AnyRequest> for PendingService {
        type Response = serde_json::Value;
        type Error = ResponseError;
        type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: AnyRequest) -> Self::Future {
            Box::pin(pending())
        }
    }

    impl LspService for PendingService {
        fn notify(&mut self, _: AnyNotification) -> ControlFlow<async_lsp::Result<()>> {
            ControlFlow::Continue(())
        }

        fn emit(&mut self, _: AnyEvent) -> ControlFlow<async_lsp::Result<()>> {
            ControlFlow::Continue(())
        }
    }

    fn request(id: i32) -> AnyRequest {
        serde_json::from_value(json!({
            "id": id,
            "method": "workspace/symbol",
            "params": {"query": ""}
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn pending_requests_do_not_block_admission_and_remain_cancellable() {
        let mut service = CancellationLayer.layer(PendingService);
        poll_fn(|context| service.poll_ready(context)).await.unwrap();
        let first = service.call(request(1));
        poll_fn(|context| service.poll_ready(context)).await.unwrap();
        let _second = service.call(request(2));

        let cancellation = serde_json::from_value(json!({
            "method": "$/cancelRequest",
            "params": {"id": 1}
        }))
        .unwrap();
        assert!(service.notify(cancellation).is_continue());

        let error = first.await.unwrap_err();
        assert_eq!(error.code, ErrorCode::REQUEST_CANCELLED);
    }
}
