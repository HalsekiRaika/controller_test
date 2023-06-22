#![allow(unused)]

/// A.k.a Infrastructure Layer
pub mod driver {
    use crate::kernel::{Repository, Data};

    #[derive(Clone)]
    pub struct Pool;
    
    #[derive(Clone)]
    pub struct DataRepository(pub Pool);

    #[async_trait::async_trait]
    impl Repository for DataRepository {
        async fn create(&self, data: &Data) -> Result<(), u64> {
            println!("[driver] : {:?}", data);
            Ok(())
        }
    }
}

/// A.k.a Domain Layer
pub mod kernel {
    #[derive(Debug, Clone, destructure::Destructure)]
    pub struct Data {
        id: String,
        name: String,
    }

    impl Data {
        pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
            Self { id: id.into(), name: name.into() }
        }
    }

    #[async_trait::async_trait]
    pub trait Repository: 'static + Send + Sync {
        async fn create(&self, data: &Data) -> Result<(), u64>;
    }

    pub trait DependOnRepository: 'static + Send + Sync {
        type Repository: Repository;
        fn repository(&self) -> &Self::Repository;
    }
}

/// A.k.a UseCase Layer
pub mod application {
    use crate::kernel::{DependOnRepository, Repository, Data, DestructData};

    #[derive(Debug, Clone)]
    pub struct DataDto {
        pub id: String,
        pub name: String
    }

    impl From<Data> for DataDto {
        fn from(value: Data) -> Self {
            let DestructData {
                id,
                name
            } = value.into_destruct();
            Self { id, name }
        }
    }

    #[async_trait::async_trait]
    pub trait CreateDataService: 'static + Send + Sync
        + DependOnRepository
    {
        async fn create(&self, obj: DataDto) -> Result<DataDto, u64> {
            let DataDto { id, name } = obj;
            let data = Data::new(id, name); 
            self.repository().create(&data).await?;
            Ok(data.into())
        }
    }

    // Default Impl
    impl<T> CreateDataService for T
        where T: DependOnRepository {}

    pub trait DependOnCreateDataService: 'static + Send + Sync {
        type CreateDataService: CreateDataService;
        fn create_simple_data_service(&self) -> &Self::CreateDataService;
    }
}

/// A.k.a DI Container
pub mod inject {
    use crate::{
        kernel::{DependOnRepository, Repository},
        driver::{DataRepository, Pool}, 
        application::DependOnCreateDataService, 
    };

    pub struct Handler {
        repo: DataRepository
    }
    impl Handler {
        pub fn init() -> Self {
            Self { repo: DataRepository(Pool) }
        }
    }
    impl DependOnRepository for Handler {
        type Repository = DataRepository;
        fn repository(&self) -> &Self::Repository {
            &self.repo
        }
    }
    impl DependOnCreateDataService for Handler {
        type CreateDataService = Self;
        fn create_simple_data_service(&self) -> &Self::CreateDataService {
            self
        }
    }
}

/// A.k.a Presentation Layer
pub mod adaptor {
    use std::{marker::PhantomData, future::IntoFuture};

    use crate::application::DataDto;

    pub trait InPort<I>: 'static + Sync + Send {
        type Dto;
        fn emit(&self, input: I) -> Self::Dto;
    }

    pub trait OutPort<I>: 'static + Sync + Send {
        type ViewModel;
        fn emit(&self, input: I) -> Self::ViewModel;
    }

    #[derive(Debug)]
    pub struct PresentationalDataA {
        id: String,
        name: String
    }
    
    pub struct PresenterA;
    
    impl OutPort<Result<DataDto, u64>> for PresenterA {
        type ViewModel = Result<PresentationalDataA, u64>;
        fn emit(&self, input: Result<DataDto, u64>) -> Self::ViewModel {
            match input {
                Ok(input) => {
                    Ok(PresentationalDataA {
                        id: input.id,
                        name: input.name
                    })
                },
                Err(code) => {
                    Err(code)
                }
            }
        }
    }
    
    pub struct PresenterB;
    
    impl OutPort<Result<DataDto, u64>> for PresenterB {
        type ViewModel = Result<String, u64>;
        fn emit(&self, input: Result<DataDto, u64>) -> Self::ViewModel {
            match input {
                Ok(input) => {
                    Ok(format!("{:?}", input))
                },
                Err(code) => {
                    Err(code)
                }
            }
        }
    }


    pub struct _Controller<T, P, I, D, O> {
        transformer: T,
        presenter: P,
        _in: PhantomData<I>,
        _trans: PhantomData<D>,
        _out: PhantomData<O>
    }

    impl<T, P, I, D, O> _Controller<T, P, I, D, O>
        where T: InPort<I, Dto = D>,
              P: OutPort<O>
    {
        pub fn new(transformer: T, presenter: P) -> Self {
            Self { transformer, presenter, _in: PhantomData, _trans: PhantomData, _out: PhantomData }
        }

        pub fn transform(self, input: I) -> Transformed<T, P, I, D, O> {
            Transformed { trans_input: self.transformer.emit(input), controller: self, _in: PhantomData, _out: PhantomData }
        }

        fn present(self) -> P {
            self.presenter
        }
    }

    pub struct Transformed<T, P, I, D, O> {
        controller: _Controller<T, P, I, D, O>,
        trans_input: D,
        _in: PhantomData<I>,
        _out: PhantomData<O>
    }

    impl<T, P, I, D, O> Transformed<T, P, I, D, O>
        where T: InPort<I, Dto = D>,
              P: OutPort<O>
    {
        pub async fn handle<F, Fut>(self, f: F) -> P::ViewModel
            where F: FnOnce(D) -> Fut,
                  Fut: IntoFuture<Output = O>
        {
            self.controller.present().emit(f(self.trans_input).await)
        }
    }


    pub struct Controller<P, D> {
        presenter: P,
        _presenter_input: PhantomData<D>
    }

    impl<P: OutPort<D>, D> Controller<P, D> {
        pub fn new(presenter: P) -> Self {
            Self { presenter, _presenter_input: PhantomData }
        }

        pub fn capture<R: Into<N>, N>(self, input: R) -> Captured<R, N, D, P> {
            Captured { controller: self, input, _need: PhantomData, _conv: PhantomData }
        }
    }

    pub struct Captured<R, N, D, P> {
        controller: Controller<P, D>,
        input: R,
        _need: PhantomData<N>,
        _conv: PhantomData<D>
    }
    
    impl<R, N, D, P> Captured<R, N, D, P>
        where R: Into<N>,
              P: OutPort<D>
    {
        pub async fn handle<F, Fut>(self, f: F) -> P::ViewModel
            where F: Fn(N) -> Fut,
                  Fut: IntoFuture<Output = D>
        {
            self.controller.presenter.emit(f(self.input.into()).await)
        }
    }
}

use std::future::IntoFuture;

use adaptor::{_Controller as ControllerA, Controller as ControllerB, InPort, PresenterA, PresenterB};
use application::{DependOnCreateDataService, CreateDataService};
use inject::Handler;

use crate::application::DataDto;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let handler = Handler::init();

    #[derive(Clone)]
    struct UserInputForm {
        pub id: String,
        pub name: String
    }

    pub struct TransformerA;

    impl InPort<UserInputForm> for TransformerA {
        type Dto = DataDto;
        fn emit(&self, input: UserInputForm) -> Self::Dto {
            Self::Dto {
                id: input.id,
                name: input.name
            }
        }
    }

    impl From<UserInputForm> for DataDto {
        fn from(value: UserInputForm) -> Self {
            Self {
                id: value.id,
                name: value.name
            }
        }
    }

    let input = UserInputForm {
        id: "abc123".to_string(),
        name: "test man".to_string()
    };

    let res = ControllerA::new(TransformerA, PresenterA)
        .transform(input.clone())
        .handle(|input| async { // <- ここで型が推論される
            handler.create_simple_data_service()
                .create(input)
                .await
        }).await;
    println!("{:?}", res);

    let res = ControllerA::new(TransformerA, PresenterB)
        .transform(input.clone())
        .handle(|input| async {
            handler.create_simple_data_service()
                .create(input)
                .await
        }).await;
    println!("{:?}", res);

    let res = ControllerB::new(PresenterA)
        .capture(input.clone())
        .handle(|input| async {
            handler.create_simple_data_service()
                .create(input) // <- ここで型が推論される
                .await
        }).await;
    println!("{:?}", res);
    
    let res = ControllerB::new(PresenterB)
        .capture(input)
        .handle(|input| async {
            handler.create_simple_data_service()
                .create(input)
                .await  
        }).await;
    println!("{:?}", res);

    Ok(())
}